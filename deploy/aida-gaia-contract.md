# What GAIA offers WISC/AIDA, and where AIDA's code lives

## The rule

**AIDA's source is not kept here and is not patched from here.** It lives in
`/home/j/src/widefield-star-calibrator` (served at `/aida/`, and standalone at
juha.no/aida), with a copy at `/mnt/data/juha/gaia/aida-code`. It has to keep
working as a standalone calibrator *and* as GAIA's calibrator for the realtime
and event viewers, and it is edited in its own tree.

This repository briefly carried patches against `app.js` and scripts to apply
them. That was a second source of truth for one file, which is the fastest way
to a fork nobody can maintain. They are gone; the content is still in git
history (`ab3235e`, `a6a2b3e`) if anyone wants it, and the episode below says
why it should be re-derived rather than replayed.

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

A patch adding `proposal=1` support was applied to `app.js` at 15:07 and was
gone by 15:23 -- the file was edited again in its own tree and the change did
not survive. Nothing broke: AIDA parsed, served, and `loadGaiaSourceImage`,
`loadGaiaEventImage` and `sendGaiaCalibration` were all intact throughout.

But it could have. Two parties editing one unversioned file, one of them through
patches kept in a different repository, is how a calibrator serving two products
stops being maintainable.

**AIDA is under no version control at all** -- neither copy is a git repository.
That is the deeper problem, and it is worth fixing before anything else: with a
repository, a change like this is a branch and a review, the two copies cannot
silently diverge, and a lost edit is recoverable instead of merely gone.
