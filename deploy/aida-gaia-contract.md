# What GAIA offers WISC/AIDA, and where AIDA's code lives

## The rule

**AIDA's source is not kept here and is not patched from here.** It is its own
project under its own version control:

    git@github.com:jvierine/widefield-star-calibrator.git   (branch main)

checked out at `/mnt/data/juha/gaia/aida-code` and deployed to
`/home/j/src/widefield-star-calibrator`, which is what `/aida/` serves here and
what juha.no/aida serves standalone. The deployment has no `.git` of its own,
which is as it should be: it is a deploy target, not a second checkout.

Changes to AIDA therefore belong as commits on that repository, reviewed there.
This repository briefly carried patches against its `app.js` and scripts to
apply them, which made a second source of truth for one file -- the fastest way
to a fork nobody can maintain. They are gone; the content is still in this
repository's history (`ab3235e`, `a6a2b3e`) if anyone wants to re-derive it
against the current file.

What GAIA owns is the **contract**: an endpoint, and what it serves. What AIDA
does with it is AIDA's.

## The contract

    GET /gaia/api/sources/{source_id}/calibration/refit-proposal

Read-only; it writes nothing on either side. It returns the richest measured
frame of the last night observed, the stars identified in it, and the lens
model fitted through them, as a proposal for a person to check.

| field | meaning |
| --- | --- |
| `image_id`, `observation_utc`, `width`, `height` | the frame the fit was made on |
| `optmod` | the WISC model number, unchanged by the fit |
| `optpar` | the eight free parameters, in the order AIDA's controls take |
| `optpar_with_optmod` | the same with the model number in front |
| `seed_optpar_with_optmod` | the model it started from |
| `residual_px_before`, `residual_px_after` | RMS against the identified stars |
| `matches[]` | `image_x`, `image_y`, `star_key`, `ra_hours`, `dec_deg`, `mag`, `azimuth_deg`, `zenith_deg`, `residual_px`, `residual_dx`, `residual_dy` |

Zenith angle rather than elevation, since that is what AIDA's `state.matches`
stores. Residuals are under the *proposed* model, so a star it places badly is
visible before the model is accepted.

GAIA's lens-drift panel links to
`/aida/?gaia=1&source_id=...&image_id=...&proposal=1`. **GAIA does not require
AIDA to do anything with `proposal=1`.** Without support for it AIDA loads the
frame exactly as it always has, and the stars and the proposed model simply do
not cross over. Nothing degrades.

Star names are not in the data GAIA holds: the catalogue in use is
`tycho2_mag8.bin.gz`, whose records are right ascension, declination and
magnitude, with no names anywhere. AIDA already carries the Yale bright-star
list with names, so if names are wanted on these identifications the lookup
belongs there, matching on position.

## What happened on 2026-09-22, and the lesson

A patch adding `proposal=1` support was applied directly to the *deployed*
`app.js` at 15:07 and was gone by 15:23, replaced when Juha's own work landed --
commit `53c9453`, "Couple AIDA to GAIA event calibrations". Nothing broke at any
point: AIDA parsed, served, and `loadGaiaSourceImage`, `loadGaiaEventImage` and
`sendGaiaCalibration` were intact throughout.

The change vanished because it was made to a deployed checkout rather than
committed to the repository. That is the mechanism working, not failing: an
untracked edit to a deploy target is supposed to be discarded by the next
deploy. The lesson is simply that AIDA changes go through
`jvierine/widefield-star-calibrator` like any other change to it, as a commit
and a review, and never as a patch applied from a neighbouring project.

The `proposal=1` support is therefore still unbuilt on AIDA's side, and GAIA
does not need it to be. If it is wanted, it should be raised as a pull request
on that repository by someone with an identity there.
