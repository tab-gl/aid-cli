/*
 * Ace ID terminal palette.
 *
 * Keep terminal text in a neutral near-white hierarchy instead of
 * using one white value everywhere. Colour is reserved for the two
 * status exceptions — success and errors — so those stay meaningful
 * without tinting ordinary output.
 */

function supportsColor(stream) {
  if (process.env.NO_COLOR) return false;
  if (process.env.TERM === "dumb") return false;
  if (!stream.isTTY) return false;
  return true;
}

const enabled = supportsColor(process.stdout);
const VOID_BLACK = [0, 0, 0];
const PRIMARY = [250, 250, 250];
const STRONG = [244, 244, 245];
const DEFAULT = [228, 228, 231];
const SECONDARY = [212, 212, 216];
const MUTED = [161, 161, 170];
const DIMMED = [113, 113, 122];
const PURE_WHITE = [255, 255, 255];
const ESC = String.fromCharCode(27);

const ansi = (code) => `${ESC}[${code}m`;
const rgb = (value) => `${ESC}[38;2;${value.join(";")}m`;
const fgPrimary = rgb(PRIMARY);
const fgStrong = rgb(STRONG);
const fgDefault = rgb(DEFAULT);
const fgSecondary = rgb(SECONDARY);
const fgMuted = rgb(MUTED);
const fgDimmed = rgb(DIMMED);
const fgPureWhite = rgb(PURE_WHITE);
const bg = ansi(`48;2;${VOID_BLACK.join(";")}`);
const reset = ansi("0");

const wrap = (open) => (s) =>
  enabled ? `${open}${s}${reset}` : String(s);

export const primary = (s) => wrap(fgPrimary)(s);
export const bold = (s) => wrap(`${fgStrong}${ansi("1")}`)(s);
export const foreground = (s) => wrap(fgDefault)(s);
export const secondary = (s) => wrap(fgSecondary)(s);
export const dim = (s) => wrap(fgMuted)(s);
export const faint = (s) => wrap(fgDimmed)(s);
export const white = (s) => wrap(fgPureWhite)(s);

export const red = (s) => wrap(ansi("38;2;225;90;90"))(s);
export const green = (s) => wrap(ansi("38;2;115;190;145"))(s);
export const gray = dim;

export const inverse = (s) =>
  enabled ? `${bg}${fgStrong}${s}${reset}` : String(s);

export const background = (s) =>
  enabled ? `${bg}${s}${reset}` : String(s);

export const colorsEnabled = enabled;

export const palette = {
  voidBlack: "#000000",
  primary: "#FAFAFA",
  strong: "#F4F4F5",
  foreground: "#E4E4E7",
  secondary: "#D4D4D8",
  muted: "#A1A1AA",
  dim: "#71717A",
  white: "#FFFFFF"
};
