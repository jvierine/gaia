// Where whole-day playback is, in time rather than in frames.
//
// The published overview is a sequence of discrete images, one per published
// minute, and the globe must show one of them at a time. The Earth underneath
// them is not discrete: the terminator sweeps, and under sun lock the whole
// globe turns. Driving both from the image index makes the rotation advance
// once per image and stand still in between. These two functions separate the
// question "which image" from the question "what time is it", so the first can
// step and the second cannot.

/// Position within one loop, in [0, 1), for a playback clock that has been
/// running `elapsed` milliseconds with a loop of `duration`.
export function loopPosition(elapsed: number, duration: number): number {
  if (!(duration > 0) || !Number.isFinite(elapsed)) return 0;
  const wrapped = elapsed % duration;
  return (wrapped < 0 ? wrapped + duration : wrapped) / duration;
}

/// The instant playback is at, interpolated across the gap between image times.
/// `times` must be ascending.
///
/// The final segment carries on at the spacing of the one before it rather than
/// holding at the last image. Holding would freeze the clock for the last
/// 1/N of every loop, which is the very stutter this module exists to remove,
/// and the images are evenly spaced so the previous gap is the right step.
export function clockAt(times: number[], position: number): number | null {
  if (!times.length) return null;
  if (times.length === 1) return times[0];
  const place = Math.min(times.length - 1e-9, Math.max(0, position) * times.length);
  const index = Math.min(times.length - 1, Math.floor(place));
  const span = index + 1 < times.length
    ? times[index + 1] - times[index]
    : times[index] - times[index - 1];
  return times[index] + (place - index) * span;
}

/// Where in the loop to resume so that playback carries on from `from` instead
/// of rewinding. Returns 0 when there is nowhere to carry on from: an empty
/// day, an unreadable time, or a time outside the published day, which is where
/// the live edge sits.
///
/// This is what makes a speed change a change of speed. The event carrying a new
/// rate reaches the player exactly like a fresh play, so without a resume point
/// every adjustment would restart the run.
export function resumePosition(times: number[], from: number | null): number {
  if (from == null || !Number.isFinite(from) || times.length < 2) return 0;
  const first = times[0];
  const last = times[times.length - 1];
  if (from < first || from > last) return 0;
  let nearest = 0;
  for (let i = 1; i < times.length; i++) {
    if (Math.abs(times[i] - from) < Math.abs(times[nearest] - from)) nearest = i;
  }
  return nearest / times.length;
}
