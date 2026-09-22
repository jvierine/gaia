# WISC/AIDA: accept a GAIA refit proposal

GAIA's lens-drift panel no longer writes a refitted model into the calibration
list. It computes a proposal — the richest frame, the stars identified in it,
and the best-fitting lens parameters — and hands it to WISC/AIDA so a person can
inspect the identifications first. Some automatic fits are dubious (a hot pixel,
a satellite, a star pulled onto its neighbour), and a model fitted through them
should not become a calibration unseen.

## Status, 2026-09-22: not applied, and not to be applied by GAIA

The proposal patch was applied at 15:07 and was **gone by 15:23** -- AIDA's
`app.js` was edited again and the change did not survive. Nothing was broken by
that: the file parses, `/aida/` serves, and `loadGaiaSourceImage`,
`loadGaiaEventImage` and `sendGaiaCalibration` are all intact, so AIDA works
standalone on juha.no/aida and as GAIA's calibrator exactly as before.

It has not been re-applied, and should not be from the GAIA side. AIDA is
Juha's, it is actively edited there, and two parties writing one file without
coordinating is precisely how a calibrator that has to serve two products gets
broken. The installers now **refuse** unless `app.js` is byte-for-byte the
version the patch was verified against, so they cannot quietly re-inject into a
file that has moved on.

GAIA does not depend on the patch. The lens-drift panel links to
`/aida/?gaia=1&source_id=...&image_id=...&proposal=1`; without the patch AIDA
ignores `proposal=1` and loads the frame as it always has. The stars and the
proposed model simply do not cross over.

If the handoff is wanted, `deploy/aida-proposal.patch` and
`deploy/aida-names.patch` are the changes, for Juha to review, regenerate
against the current file and apply.

## Applying it

A verified patch and an installer now sit beside this file:
`deploy/aida-proposal.patch` and `deploy/apply-aida-proposal.sh` (also staged
at `/mnt/data/juha/gaia/staging/`). The patch was generated against the served
`app.js` as of 2026-09-22 11:30, is purely additive (58 lines added, none
removed), and both the original and the patched file parse under `node --check`.
`patch --dry-run` applies cleanly.

    sudo sh /mnt/data/juha/gaia/staging/apply-aida-proposal.sh

The script refuses to do anything twice, backs the file up first, and restores
the backup if the patched file does not parse -- a syntax error in `app.js`
takes the whole calibrator down and would otherwise only show in a browser
console.

**It has to be applied by `j` or root.** AIDA lives in
`/home/j/src/widefield-star-calibrator` (served at `/aida/`, with an identical
copy at `/mnt/data/juha/gaia/aida-code`), which `bgu001` cannot write. It is
also a separate repository from `jvierine/gaia`, so it does not arrive with a
GAIA install. **It is untested**, for the same reason.

## What GAIA now serves

    GET /gaia/api/sources/{source_id}/calibration/refit-proposal

Read-only; it writes nothing. The fields AIDA needs:

| field | meaning |
| --- | --- |
| `image_id`, `observation_utc`, `width`, `height` | the frame the fit was made on |
| `optmod` | the WISC model number, unchanged by the fit |
| `optpar` | the eight free parameters, in the order AIDA's own controls take |
| `optpar_with_optmod` | the same with the model number in front |
| `seed_optpar_with_optmod` | the model it started from, for comparison |
| `residual_px_before`, `residual_px_after` | RMS against the identified stars |
| `matches[]` | `image_x`, `image_y`, `star_key`, `ra_hours`, `dec_deg`, `mag`, `azimuth_deg`, `zenith_deg`, `residual_px`, `residual_dx`, `residual_dy` |

`zenith_deg` rather than elevation, since that is what `state.matches` stores.
The residuals are under the *proposed* model, so a star it places badly is
visible before the model is accepted.

## The change to `js/app.js`

GAIA opens `/aida/?gaia=1&source_id=…&image_id=…&proposal=1`. The extra
`proposal=1` is the only new parameter; without it nothing changes.

**1.** In `loadGaiaSourceImage`, in the callback passed to `loadImageFile`,
after the line that sets `controls.altM.value`, add:

```js
            if (params.get("proposal") === "1") { void applyGaiaProposal(sourceId); }
```

**2.** Add this function beside `loadGaiaSourceImage`. It reuses
`applyFitVector`, which already writes a parameter vector into the controls and
`state.modelOptpar`, so the proposed model arrives the same way a fitted one
does:

```js
    async function applyGaiaProposal(sourceId) {
        try {
            const response = await fetch(
                `/gaia/api/sources/${encodeURIComponent(sourceId)}/calibration/refit-proposal`,
                {cache: "no-store"});
            if (!response.ok) {
                throw new Error(await response.text() || `server returned ${response.status}`);
            }
            const proposal = await response.json();
            if (Number.isFinite(Number(proposal.optmod))) {
                controls.optmod.value = String(proposal.optmod);
            }
            const optpar = Array.isArray(proposal.optpar) ? proposal.optpar.map(Number) : [];
            if (optpar.length >= 8) applyFitVector(optpar);
            state.matches = (Array.isArray(proposal.matches) ? proposal.matches : [])
                .map((match, index) => ({
                    id: Number.isFinite(Number(match.id)) ? Number(match.id) : index + 1,
                    image: {
                        x: Number(match.image_x) || 0,
                        y: Number(match.image_y) || 0,
                        // Marked as automatic so it is obvious which pairings a
                        // person has not yet vouched for.
                        method: "gaia-auto",
                    },
                    catalog: {
                        key: String(match.star_key || `gaia-${index}`),
                        name: String(match.star_key || ""),
                        raHours: Number(match.ra_hours) || 0,
                        decDeg: Number(match.dec_deg) || 0,
                        mag: Number(match.mag) || 0,
                        az: Number(match.azimuth_deg) || 0,
                        ze: Number(match.zenith_deg) || 0,
                    },
                }));
            const before = Number(proposal.residual_px_before);
            const after = Number(proposal.residual_px_after);
            state.fitMessage = `GAIA: ${state.matches.length} automatically identified stars and a `
                + `proposed lens model loaded (${before.toFixed(2)} to ${after.toFixed(2)} px RMS). `
                + `Check the identifications, discard any that are wrong, refit, and send the `
                + `calibration back.`;
            render();
        } catch (error) {
            state.fitMessage = `GAIA proposal load failed: `
                + `${error && error.message ? error.message : error}`;
            render();
        }
    }
```

## What to check once it is in

- The image, timestamp and station load as they already do.
- The eight parameter controls hold the proposed values, and `optmod` is
  unchanged from the calibration the fit started from.
- The identified stars appear as pairings and can be deleted individually —
  that is the whole point of the handoff.
- The existing "send the calibration back" path still writes a calibration, and
  that remains the only way a refitted model reaches the list.
