/*
 * Ace ID terminal palette.
 *
 * Void Black: terminal/UI background.
 * F2F2F2: primary terminal foreground.
 */

function supportsColor(stream) {
  if (process.env.NO_COLOR) return false;
  if (process.env.TERM === "dumb") return false;
  if (!stream.isTTY) return false;
  return true;
}

const enabled = supportsColor(process.stdout);
const VOID_BLACK = [0, 0, 0];
const F2F2F2 = [242, 242, 242];

const ansi = (code) => `\\x1b[${code}m`;
const fg = ansi(`38;2;${F2F2F2.join(";")}`);
const bg = ansi(`48;2;${VOID_BLACK.join(";")}`);
const reset = ansi("0");

const wrap = (open) => (s) =>
  enabled ? `${open}${s}${reset}` : String(s);

export const dim = (s) => wrap(fg)(s);
export const bold = (s) => wrap(`${fg}${ansi("1")}`)(s);
export const red = (s) => wrap(`${ansi("38;2;225;90;90")}`)(s);
export const green = (s) => wrap(`${ansi("38;2;115;190;145")}`)(s);
export const yellow = (s) => wrap(`${ansi("38;2;220;185;105")}`)(s);
export const blue = (s) => wrap(`${ansi("38;2;95;155;190")}`)(s);
export const gray = dim;

export const inverse = (s) =>
  enabled ? `${bg}${fg}${s}${reset}` : String(s);

export const foreground = (s) =>
  enabled ? `${fg}${s}${reset}` : String(s);

export const background = (s) =>
  enabled ? `${bg}${s}${reset}` : String(s);

export const colorsEnabled = enabled;

export const palette = {
  voidBlack: "#000000",
  foreground: "#F2F2F2"
};
