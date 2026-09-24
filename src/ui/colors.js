/*
 * Ace ID terminal palette.
 *
 * Void Black: terminal/UI background.
 * Moonstone: primary foreground.
 */

function supportsColor(stream) {
  if (process.env.NO_COLOR) return false;
  if (process.env.TERM === "dumb") return false;
  if (!stream.isTTY) return false;
  return true;
}

const enabled = supportsColor(process.stdout);
const VOID_BLACK = [0, 0, 0];
const MOONSTONE = [115, 169, 194];

const rgb = (r, g, b, foreground = true) =>
  `\\x1b[${foreground ? 38 : 48};2;${r};${g};${b}m`;
const reset = "\\x1b[0m";

const wrap = (open) => (s) =>
  enabled ? `${open}${s}${reset}` : String(s);

const moonstone = `\\x1b[38;2;${MOONSTONE[0]};${MOONSTONE[1]};${MOONSTONE[2]}m`;
const voidBlack = `\\x1b[48;2;${VOID_BLACK[0]};${VOID_BLACK[1]};${VOID_BLACK[2]}m`;

export const dim = (s) =>
  enabled ? `\\x1b[38;2;90;120;135m${s}${reset}` : String(s);
export const bold = wrap(`${moonstone}\\x1b[1m`);
export const red = wrap(`${rgb(225, 90, 90)}`);
export const green = wrap(`${rgb(115, 190, 145)}`);
export const yellow = wrap(`${rgb(220, 185, 105)}`);
export const blue = wrap(`${rgb(95, 155, 190)}`);
export const gray = dim;

export const inverse = (s) =>
  enabled ? `${voidBlack}${moonstone}${s}${reset}` : String(s);

// Primary terminal foreground/background helpers.
export const fg = (s) =>
  enabled ? `${moonstone}${s}${reset}` : String(s);
export const bg = (s) =>
  enabled ? `${voidBlack}${s}${reset}` : String(s);

export const colorsEnabled = enabled;
export const palette = {
  voidBlack: "#000000",
  moonstone: "#73A9C2"
};
