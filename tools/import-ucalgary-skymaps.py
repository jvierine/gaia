#!/usr/bin/env python3
"""Convert UCalgary SRS per-pixel skymaps to validated AIDA/WISC lens files.

The upstream IDL save files remain the authoritative calibration.  This tool
fits the compact AIDA model used by GAIA, validates it on pixels excluded from
the fit, writes a normal AIDA-compatible HDF5 file, and optionally registers it
in the GAIA SQLite database.
"""

from __future__ import annotations

import argparse
import datetime as dt
import hashlib
import html.parser
import json
import math
import os
from pathlib import Path
import re
import sqlite3
import tempfile
import time
from urllib.parse import urljoin
from urllib.request import Request, urlopen

import h5py
import numpy as np
from scipy.io import readsav
from scipy.optimize import least_squares
from scipy.spatial.transform import Rotation


USER_AGENT = "GAIA UCalgary calibration importer/1.0 (contact: gaia@juha.no)"
ROOTS = {
    "trexrgb": ("https://data.phys.ucalgary.ca/sort_by_project/TREx/RGB/skymaps/", "rgb"),
    "trexnir": ("https://data.phys.ucalgary.ca/sort_by_project/TREx/NIR/skymaps/", "nir"),
    "trexblue": ("https://data.phys.ucalgary.ca/sort_by_project/TREx/blueline/skymaps/", "blue"),
    "rego": ("https://data.phys.ucalgary.ca/sort_by_project/GO-Canada/REGO/skymap/", "rego"),
    "smileasi": ("https://data.phys.ucalgary.ca/sort_by_project/SMILE/asi/l0/skymaps/", "smile"),
}


class Links(html.parser.HTMLParser):
    def __init__(self) -> None:
        super().__init__()
        self.hrefs: list[str] = []

    def handle_starttag(self, tag: str, attrs: list[tuple[str, str | None]]) -> None:
        if tag == "a":
            href = dict(attrs).get("href")
            if href:
                self.hrefs.append(href)


def fetch(url: str) -> bytes:
    request = Request(url, headers={"User-Agent": USER_AGENT, "From": "gaia@juha.no"})
    with urlopen(request, timeout=90) as response:
        return response.read()


def links(url: str) -> list[str]:
    parser = Links()
    parser.feed(fetch(url).decode("utf-8", "replace"))
    return parser.hrefs


def source_parts(source_id: str) -> tuple[str, str]:
    match = re.fullmatch(r"ucalgary-([a-z]+)_([a-z0-9]+)_standard", source_id)
    if not match or match.group(1) not in ROOTS:
        raise ValueError(f"no UCalgary skymap family for {source_id}")
    return match.group(1), match.group(2)


def newest_skymap(source_id: str) -> tuple[str, str]:
    family, site = source_parts(source_id)
    root, prefix = ROOTS[family]
    site_url = urljoin(root, f"{site}/")
    dated = sorted(
        href for href in links(site_url)
        if re.fullmatch(rf"{re.escape(site)}_\d{{8}}/", href)
    )
    if not dated:
        raise RuntimeError(f"no calibration directories at {site_url}")
    directory = dated[-1]
    directory_url = urljoin(site_url, directory)
    saves = sorted(href for href in links(directory_url) if href.endswith(".sav"))
    preferred = [href for href in saves if href.startswith(f"{prefix}_skymap_")]
    if not preferred:
        raise RuntimeError(f"no {prefix} skymap in {directory_url}")
    valid_date = re.search(r"(\d{8})", directory).group(1)
    return urljoin(directory_url, preferred[-1]), valid_date


def skymap_arrays(payload: bytes) -> tuple[np.ndarray, np.ndarray]:
    with tempfile.NamedTemporaryFile(suffix=".sav") as temp:
        temp.write(payload)
        temp.flush()
        values = readsav(temp.name, python_dict=True)
    records = [v for v in values.values() if getattr(getattr(v, "dtype", None), "names", None)]
    if len(records) != 1:
        raise RuntimeError("expected one record-valued skymap variable")
    record = records[0].reshape(-1)[0]
    azimuth = np.asarray(record["FULL_AZIMUTH"], dtype=np.float64).squeeze()
    elevation = np.asarray(record["FULL_ELEVATION"], dtype=np.float64).squeeze()
    if azimuth.ndim != 2 or azimuth.shape != elevation.shape:
        raise RuntimeError("FULL_AZIMUTH and FULL_ELEVATION must be equal 2-D grids")
    return azimuth, elevation


def rotation_matrix(angles_deg: np.ndarray) -> np.ndarray:
    alpha, beta, gamma = np.deg2rad(angles_deg)
    ca, sa, cb, sb, cg, sg = (
        np.cos(alpha), np.sin(alpha), np.cos(beta), np.sin(beta),
        np.cos(gamma), np.sin(gamma),
    )
    return (
        np.array([[ca, 0.0, sa], [0.0, 1.0, 0.0], [-sa, 0.0, ca]])
        @ np.array([[1.0, 0.0, 0.0], [0.0, cb, sb], [0.0, -sb, cb]])
        @ np.array([[cg, -sg, 0.0], [sg, cg, 0.0], [0.0, 0.0, 1.0]])
    )


def aida_angles(matrix: np.ndarray) -> np.ndarray:
    fits = [
        least_squares(
            lambda angles: (rotation_matrix(angles) - matrix).ravel(), start,
            max_nfev=2000,
        )
        for start in ([0, 0, 0], [90, 0, 0], [-90, 0, 0],
                      [0, 90, 0], [0, -90, 0], [0, 0, 180])
    ]
    fit = min(fits, key=lambda candidate: np.linalg.norm(candidate.fun))
    if np.max(np.abs(rotation_matrix(fit.x) - matrix)) > 1e-7:
        raise RuntimeError("could not express initial rotation in AIDA angles")
    return fit.x


def model_vectors(params: np.ndarray, optmod: int, x: np.ndarray, y: np.ndarray,
                  width: int, height: int) -> np.ndarray:
    f1, f2, alpha, beta, gamma, du, dv, radial_alpha = params
    qx = ((x + 1.0) / width - 0.5 - du) / f1
    qy = ((y + 1.0) / height - 0.5 - dv) / f2
    q = np.hypot(qx, qy)
    if optmod == 2:
        theta = np.arcsin(np.clip(q, -1, 1)) / radial_alpha
    elif optmod == 3:
        theta = np.minimum(q, 1.45)
        for _ in range(12):
            cosine = np.cos(theta)
            value = (1.0 - radial_alpha) * np.tan(theta) + radial_alpha * theta - q
            slope = (1.0 - radial_alpha) / np.maximum(cosine * cosine, 1e-8) + radial_alpha
            theta -= value / slope
    elif optmod == 4:
        theta = np.power(q, 1.0 / radial_alpha)
    elif optmod == 5:
        theta = np.arctan(q) / radial_alpha
    elif optmod == 6:
        theta = 2.0 * np.arcsin(np.clip(q, -1, 1))
    elif optmod == 12:
        if radial_alpha > 1e-7:
            theta = np.arctan(q * radial_alpha) / radial_alpha
        elif radial_alpha < -1e-7:
            theta = np.arcsin(np.clip(q * radial_alpha, -1, 1)) / radial_alpha
        else:
            theta = q
    else:
        raise ValueError(f"unsupported fitted optmod {optmod}")
    inv_q = np.divide(1.0, q, out=np.zeros_like(q), where=q > 1e-12)
    camera = np.column_stack((qx * inv_q * np.sin(theta),
                              qy * inv_q * np.sin(theta), np.cos(theta)))
    camera[q <= 1e-12] = (0, 0, 1)
    return camera @ rotation_matrix(np.array([alpha, beta, gamma])).T


def angular_errors(predicted: np.ndarray, reference: np.ndarray) -> np.ndarray:
    return np.rad2deg(np.arccos(np.clip(np.sum(predicted * reference, axis=1), -1, 1)))


def initial_guess(x: np.ndarray, y: np.ndarray, sky: np.ndarray,
                  width: int, height: int) -> tuple[np.ndarray, tuple[int, int]]:
    u = (x + 1.0) / width - 0.5
    v = (y + 1.0) / height - 0.5
    radius = np.hypot(u, v)
    best = None
    for sign_x in (-1, 1):
        for sign_y in (-1, 1):
            qx, qy = u / (sign_x * 0.32), v / (sign_y * 0.32)
            q = np.hypot(qx, qy)
            camera = np.column_stack((
                np.divide(qx, q, out=np.zeros_like(q), where=q > 0) * np.sin(q),
                np.divide(qy, q, out=np.zeros_like(q), where=q > 0) * np.sin(q),
                np.cos(q),
            ))
            rotation, rssd = Rotation.align_vectors(sky, camera)
            candidate = (rssd, aida_angles(rotation.as_matrix()), (sign_x, sign_y))
            if best is None or candidate[0] < best[0]:
                best = candidate
    return best[1], best[2]


def fit_lens(azimuth: np.ndarray, elevation: np.ndarray,
             width: int, height: int) -> tuple[int, np.ndarray, dict[str, float]]:
    source_height, source_width = azimuth.shape
    yy, xx = np.mgrid[:source_height, :source_width]
    valid = (np.isfinite(azimuth) & np.isfinite(elevation) &
             (elevation >= 5.0) & (elevation <= 90.0))
    # Express UCalgary pixel centres in the current realtime image geometry.
    x = (xx[valid] + 0.5) * width / source_width - 0.5
    y = (yy[valid] + 0.5) * height / source_height - 0.5
    az = np.deg2rad(azimuth[valid])
    el = np.deg2rad(elevation[valid])
    sky = np.column_stack((np.cos(el) * np.sin(az), np.cos(el) * np.cos(az), np.sin(el)))
    # A deterministic spatial split prevents neighbouring fit pixels from
    # masquerading as an independent validation.
    validation = ((xx[valid] // 8 + yy[valid] // 8) % 5 == 0)
    train = ~validation
    stride = max(1, int(math.ceil(np.count_nonzero(train) / 5000)))
    train_indices = np.flatnonzero(train)[::stride]
    init_angles, signs = initial_guess(x[train_indices], y[train_indices],
                                       sky[train_indices], width, height)
    candidates = []
    model_ranges = {
        2: (0.15, 1.5, 0.7),
        3: (0.0, 1.0, 0.8),
        4: (0.3, 2.0, 1.0),
        5: (0.15, 1.5, 0.7),
        6: (0.99, 1.01, 1.0),
        12: (-2.0, 2.0, 0.0),
    }
    for optmod, (radial_min, radial_max, radial_start) in model_ranges.items():
        start = np.array([signs[0] * 0.32, signs[1] * 0.32,
                          *init_angles, 0.0, 0.0, radial_start])
        f1_bounds = (0.04, 2.0) if signs[0] > 0 else (-2.0, -0.04)
        f2_bounds = (0.04, 2.0) if signs[1] > 0 else (-2.0, -0.04)
        lower = [f1_bounds[0], f2_bounds[0], -720, -720, -720, -0.35, -0.35, radial_min]
        upper = [f1_bounds[1], f2_bounds[1], 720, 720, 720, 0.35, 0.35, radial_max]
        fit = least_squares(
            lambda p: (model_vectors(p, optmod, x[train_indices], y[train_indices],
                                     width, height) - sky[train_indices]).ravel(),
            start, bounds=(lower, upper), loss="soft_l1", f_scale=0.002,
            max_nfev=2000,
        )
        errors = angular_errors(
            model_vectors(fit.x, optmod, x[validation], y[validation], width, height),
            sky[validation],
        )
        summary = {
            "validation_rms_deg": float(np.sqrt(np.mean(errors ** 2))),
            "validation_median_deg": float(np.median(errors)),
            "validation_p95_deg": float(np.percentile(errors, 95)),
            "validation_max_deg": float(np.max(errors)),
            "validation_pixels": int(errors.size),
            "fit_pixels": int(train_indices.size),
        }
        candidates.append((summary["validation_rms_deg"], optmod, fit.x, summary))
    _, optmod, params, summary = min(candidates, key=lambda item: item[0])
    if summary["validation_rms_deg"] > 0.35 or summary["validation_p95_deg"] > 0.75:
        raise RuntimeError(f"AIDA fit failed validation: {summary}")
    return optmod, params, summary


def latest_dimensions(db_path: Path | None, source_id: str,
                      fallback: tuple[int, int]) -> tuple[int, int]:
    if db_path is None or not db_path.exists():
        return fallback
    with sqlite3.connect(db_path) as db:
        row = db.execute(
            "SELECT width,height FROM images WHERE source_id=? AND width>0 AND height>0 "
            "ORDER BY observation_utc DESC LIMIT 1", (source_id,),
        ).fetchone()
    return (int(row[0]), int(row[1])) if row else fallback


def write_hdf5(path: Path, source: dict, source_url: str, valid_date: str,
               width: int, height: int, optmod: int, params: np.ndarray,
               summary: dict[str, float]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(f".{path.name}.{os.getpid()}.tmp")
    vector = np.r_[optmod, params]
    with h5py.File(temp, "w") as h:
        h.create_dataset("wisc_optpar", data=params)
        h.create_dataset("wisc_optpar_with_optmod", data=vector)
        h.create_dataset("best_fit_wisc_optpar_with_optmod", data=vector)
        h.create_dataset("ucalgary_validation_summary", data=np.array([
            summary["validation_rms_deg"], summary["validation_median_deg"],
            summary["validation_p95_deg"], summary["validation_max_deg"],
        ]))
        h.attrs["creator"] = "GAIA UCalgary skymap to AIDA converter"
        h.attrs["generated_utc"] = dt.datetime.now(dt.timezone.utc).isoformat()
        h.attrs["image_width"] = width
        h.attrs["image_height"] = height
        h.attrs["optmod"] = optmod
        h.attrs["model_coordinates"] = "raw_image_pixel_centers"
        h.attrs["source_id"] = source["id"]
        h.attrs["site_lat_deg"] = source["latitude_deg"]
        h.attrs["site_lon_deg"] = source["longitude_deg"]
        if source.get("altitude_m") is not None:
            h.attrs["site_alt_m"] = source["altitude_m"]
        h.attrs["source_skymap_url"] = source_url
        h.attrs["source_skymap_valid_from_utc"] = f"{valid_date[:4]}-{valid_date[4:6]}-{valid_date[6:]}T00:00:00Z"
        h.attrs["validation_summary_names"] = "rms_deg median_deg p95_deg max_deg"
        h.attrs["validation_method"] = "spatially held-out UCalgary FULL_AZIMUTH/FULL_ELEVATION pixels at elevation >= 5 deg"
        h.attrs["recommended_camera_dataset"] = "/best_fit_wisc_optpar_with_optmod"
    os.replace(temp, path)


def register(db_path: Path, source_id: str, path: Path, valid_date: str,
             summary: dict[str, float], source_url: str) -> None:
    valid_from = f"{valid_date[:4]}-{valid_date[4:6]}-{valid_date[6:]}T00:00:00Z"
    digest = hashlib.sha256(source_url.encode()).hexdigest()[:24]
    calibration_id = f"ucalgary-{source_id}-{valid_date}-{digest}"
    now = dt.datetime.now(dt.timezone.utc).isoformat()
    with sqlite3.connect(db_path) as db:
        if not db.execute("SELECT 1 FROM sources WHERE id=?", (source_id,)).fetchone():
            raise RuntimeError(f"source {source_id} is not registered in {db_path}")
        db.execute(
            "INSERT INTO calibrations(id,source_id,created_utc,valid_from_utc,method,hdf5_path,residual_px,submitted_by) "
            "VALUES(?,?,?,?,?,?,NULL,?) ON CONFLICT(id) DO UPDATE SET "
            "created_utc=excluded.created_utc,hdf5_path=excluded.hdf5_path,submitted_by=excluded.submitted_by",
            (calibration_id, source_id, now, valid_from,
             "AIDA/WISC fit to UCalgary per-pixel azimuth/elevation", str(path),
             f"Automated held-out validation RMS {summary['validation_rms_deg']:.4f} deg; {source_url}"),
        )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--sources", type=Path, default=Path("sources/ucalgary.json"))
    parser.add_argument("--archive-root", type=Path, default=Path(os.environ.get("GAIA_ARCHIVE_ROOT", "work")))
    parser.add_argument("--db", type=Path, default=Path(os.environ["GAIA_DB_PATH"]) if "GAIA_DB_PATH" in os.environ else None)
    parser.add_argument("--source-id", action="append", help="import only this source (repeatable)")
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--delay", type=float, default=1.0, help="seconds between upstream requests")
    args = parser.parse_args()
    configured = json.loads(args.sources.read_text())
    wanted = set(args.source_id or [])
    sources = [item for item in configured if item["id"].startswith("ucalgary-") and
               "smileauto_" not in item["id"] and (not wanted or item["id"] in wanted)]
    if wanted - {item["id"] for item in sources}:
        raise SystemExit(f"unknown or unsupported source IDs: {sorted(wanted - {item['id'] for item in sources})}")
    failures = []
    for index, item in enumerate(sources):
        source_id = item["id"]
        try:
            url, valid_date = newest_skymap(source_id)
            time.sleep(args.delay)
            payload = fetch(url)
            azimuth, elevation = skymap_arrays(payload)
            dimensions = latest_dimensions(args.db, source_id, (azimuth.shape[1], azimuth.shape[0]))
            optmod, params, summary = fit_lens(azimuth, elevation, *dimensions)
            output = args.archive_root / "calibrations" / source_id / f"ucalgary-{valid_date}-aida.h5"
            print(json.dumps({"source_id": source_id, "skymap": url, "dimensions": dimensions,
                              "optmod": optmod, **summary, "output": str(output)}), flush=True)
            if not args.dry_run:
                write_hdf5(output, item, url, valid_date, *dimensions, optmod, params, summary)
                if args.db:
                    register(args.db, source_id, output.resolve(), valid_date, summary, url)
        except Exception as error:
            failures.append((source_id, str(error)))
            print(json.dumps({"source_id": source_id, "error": str(error)}), flush=True)
        if index + 1 < len(sources):
            time.sleep(args.delay)
    if failures:
        raise SystemExit(f"{len(failures)} calibration imports failed: {failures}")


if __name__ == "__main__":
    main()
