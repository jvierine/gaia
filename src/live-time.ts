/** Shared safety margin for acquisition and atlas publication, in both viewers. */
export const LIVE_DELAY_MINUTES = 10;
export const LIVE_DELAY_MS = LIVE_DELAY_MINUTES * 60_000;
export function liveCutoff(now = Date.now()): number {
  return Math.floor((now - LIVE_DELAY_MS) / 60_000) * 60_000;
}
export function readyFrames<T extends {at:string}>(frames:T[], now = Date.now()):T[] {
  const cutoff=liveCutoff(now);
  return frames.filter(frame=>Date.parse(frame.at)<=cutoff);
}
