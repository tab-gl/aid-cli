import { colorsEnabled } from "./colors.js";

/*
 * Screen helpers.
 *
 * We deliberately do NOT use the alternate
 * screen buffer (?1049h). Termux on Android
 * lets the user scroll the touch surface, and
 * scrolling a TUI on the alt buffer reveals
 * the normal buffer underneath — which looks
 * like the content vanished.
 *
 * Instead we clear the visible screen on the
 * normal buffer. Content stays in scrollback,
 * so finger-scrolling works as expected.
 */

const ESC = "\x1b";

function out(s) {
  process.stdout.write(s);
}

const isTTY = () => process.stdout.isTTY;

export function clear() {
  if (!isTTY()) return;

  /*
   *  H    home cursor
   *  2J   clear visible screen (scrollback preserved)
   */
  if (colorsEnabled) out(`${ESC}[48;2;0;0;0m${ESC}[38;2;242;242;242m`);
  out(`${ESC}[H`);
  out(`${ESC}[2J`);
}

export function hideCursor() {
  if (!isTTY()) return;
  out(`${ESC}[?25l`);
}

export function showCursor() {
  if (!isTTY()) return;
  out(`${ESC}[?25h`);
}

/*
 * Clear the visible screen, then
 * print a banner/help block. No alt buffer.
 *
 * Scrolling after this shows the shell history
 * that was there before, which is what a
 * mobile user expects when they scroll.
 */
export async function withScreen(fn) {
  const interactive = isTTY();

  if (interactive) {
    clear();
  }

  try {
    return await fn();
  } finally {
    if (interactive) {
      out(`${ESC}[0m`);
      showCursor();
    }
  }
}
