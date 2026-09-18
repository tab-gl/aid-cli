/*
 * Minimal ANSI helpers.
 *
 * Colors are disabled automatically when:
 *   - stdout is not a TTY (piping to a file)
 *   - NO_COLOR is set (https://no-color.org)
 *   - TERM is "dumb"
 *
 * This keeps `aidc apps list > out.txt` clean
 * and stops CI logs from filling with escapes.
 */

function supportsColor(stream) {
  if (process.env.NO_COLOR) return false;
  if (process.env.TERM === "dumb") return false;
  if (!stream.isTTY) return false;
  return true;
}

const enabled = supportsColor(process.stdout);

const wrap = (open, close) => (s) =>
  enabled ? `\x1b[${open}m${s}\x1b[${close}m` : String(s);

export const dim     = wrap("2",  "22");
export const bold    = wrap("1",  "22");
export const red     = wrap("31", "39");
export const green   = wrap("32", "39");
export const yellow  = wrap("33", "39");
export const blue    = wrap("34", "39");
export const gray    = wrap("90", "39");
export const inverse = wrap("7",  "27");

export const colorsEnabled = enabled;
