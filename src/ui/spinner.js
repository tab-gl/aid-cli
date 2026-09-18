import { colorsEnabled } from "./colors.js";

/*
 * Frame presets.
 *
 * All render correctly on Termux, Alpine, and
 * any modern terminal. No emoji — wide-char
 * width bugs on some Android terminals make
 * them unreliable.
 */
const STYLES = {
  arc:    ["◜","◝","◞","◟"],
  pulse:  ["▁","▂","▃","▄","▅","▆","▇","█","▇","▆","▅","▄","▃","▂"],
  bounce: ["⠁","⠂","⠄","⡀","⢀","⠠","⠐","⠈"],
  grow:   ["·","∙","•","●","•","∙"],
  star:   ["✶","✸","✹","✺","✹","✸"]
};

export const SPINNER_STYLES = Object.keys(STYLES);

/*
 * Register a custom spinner style globally so
 * you can use it by name:
 *
 *   registerStyle("aidc", ["◐","◓","◑","◒"]);
 *   spinner("Working…", { style: "aidc" });
 *
 * Or set AIDC_SPINNER=aidc in the environment.
 */
export function registerStyle(name, frames) {
  if (!Array.isArray(frames) || frames.length === 0) {
    throw new Error("Spinner frames must be a non-empty array.");
  }
  STYLES[name] = frames;
}

function resolveStyle(options) {
  const name =
    options.style ||
    process.env.AIDC_SPINNER ||
    "arc";

  if (options.frames) {
    return options.frames;
  }

  return STYLES[name] || STYLES.arc;
}

/*
 * Single-line terminal spinner.
 *
 * Writes to stderr by default so piped stdout
 * (`aidc apps list > out.txt`) doesn't get
 * spinner garbage mixed in.
 *
 * Falls back to plain text on non-TTY streams,
 * NO_COLOR, and TERM=dumb — animation off, but
 * final state still printed.
 */
export function spinner(text, options = {}) {
  const frames = resolveStyle(options);
  const interval = Math.max(40, options.interval || 80);
  const stream = options.stream || process.stderr;

  const isTTY =
    stream.isTTY &&
    !process.env.NO_COLOR &&
    process.env.TERM !== "dumb";

  let frameIndex = 0;
  let timer = null;
  let currentText = String(text);
  let stopped = false;

  function render() {
    const frame = frames[frameIndex % frames.length];
    frameIndex++;

    const cols = stream.columns || 80;
    const prefix = `  ${frame} `;
    const available = Math.max(1, cols - prefix.length);

    let display = currentText;
    if (display.length > available) {
      display = display.slice(0, available - 1) + "…";
    }

    const line = colorsEnabled
      ? `\x1b[2m${prefix}\x1b[22m${display}`
      : `${prefix}${display}`;

    stream.write(`\r\x1b[K${line}`);
  }

  function clearLine() {
    stream.write("\r\x1b[K");
  }

  function finish(symbol, message, color) {
    if (stopped) return api;
    stopped = true;

    if (timer) {
      clearInterval(timer);
      timer = null;
    }

    if (isTTY) clearLine();

    const c = colorsEnabled ? color : "";
    const r = colorsEnabled ? "\x1b[39m" : "";

    stream.write(`  ${c}${symbol}${r} ${message}\n`);

    return api;
  }

  const api = {
    start() {
      if (stopped) return api;

      if (isTTY) {
        render();
        timer = setInterval(render, interval);
      }

      return api;
    },

    update(newText) {
      currentText = String(newText);

      if (isTTY && !stopped) {
        render();
      }

      return api;
    },

    succeed(message = currentText) {
      return finish("✓", message, "\x1b[32m");
    },

    fail(message = currentText) {
      return finish("✗", message, "\x1b[31m");
    },

    warn(message = currentText) {
      return finish("!", message, "\x1b[33m");
    },

    info(message = currentText) {
      return finish("i", message, "\x1b[34m");
    },

    stop() {
      return finish(" ", currentText, "");
    }
  };

  return api;
}

/*
 * Convenience wrapper for a single async task:
 *
 *   const user = await withSpinner(
 *     "Loading profile…",
 *     () => apiJson("/me"),
 *     { successText: "Profile loaded" }
 *   );
 *
 * On error, the spinner fails with the error
 * message and the error is re-thrown.
 */
export async function withSpinner(text, fn, options = {}) {
  const s = spinner(text, options).start();

  try {
    const result = await fn(s);
    s.succeed(options.successText || text);
    return result;
  } catch (err) {
    s.fail(options.failText || err.message || text);
    throw err;
  }
}
