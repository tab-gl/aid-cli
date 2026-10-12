/*
 * Lightweight terminal spinner. Animations use stderr so redirected stdout
 * remains machine-readable. Non-TTY output receives only the final status.
 */
const STYLES = {
  arc: ["◜", "◝", "◞", "◟"],
  pulse: ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█", "▇", "▆", "▅", "▄", "▃", "▂"],
  bounce: ["⠁", "⠂", "⠄", "⡀", "⢀", "⠠", "⠐", "⠈"],
  grow: ["·", "∙", "•", "●", "•", "∙"],
  star: ["✶", "✸", "✹", "✺", "✹", "✸"]
};

export const SPINNER_STYLES = Object.keys(STYLES);

function validateFrames(frames) {
  if (!Array.isArray(frames) || frames.length === 0 ||
      frames.some((frame) => typeof frame !== "string" || !frame.trim())) {
    throw new Error("Spinner frames must be a non-empty array of non-empty strings.");
  }
  return [...frames];
}

export function registerStyle(name, frames) {
  if (typeof name !== "string" || !/^[a-z0-9_-]+$/i.test(name)) {
    throw new Error("Spinner style names may contain only letters, numbers, underscores, and hyphens.");
  }
  STYLES[name] = validateFrames(frames);
}

function resolveStyle(options) {
  if (options.frames !== undefined) return validateFrames(options.frames);
  const name = options.style || process.env.AIDC_SPINNER || "arc";
  return STYLES[name] || STYLES.arc;
}

export function spinner(text, options = {}) {
  const frames = resolveStyle(options);
  const intervalValue = Number(options.interval) || 80;
  const interval = Math.max(40, Math.min(2000, intervalValue));
  const stream = options.stream || process.stderr;
  const isTTY = Boolean(stream.isTTY) && process.env.TERM !== "dumb";
  const useColor = isTTY && !process.env.NO_COLOR;

  let frameIndex = 0;
  let timer = null;
  let currentText = String(text);
  let stopped = false;

  function render() {
    if (stopped || !isTTY) return;
    const frame = frames[frameIndex++ % frames.length];
    const cols = Number(stream.columns) > 0 ? Math.floor(stream.columns) : 80;
    const prefix = `  ${frame} `;
    const available = Math.max(1, cols - [...prefix].length);
    const chars = [...currentText];
    const display = chars.length > available
      ? `${chars.slice(0, Math.max(0, available - 1)).join("")}…`
      : currentText;
    const line = useColor ? `\x1b[2m${prefix}\x1b[22m${display}` : `${prefix}${display}`;
    stream.write(`\r\x1b[K${line}`);
  }

  function clearLine() {
    if (isTTY) stream.write("\r\x1b[K");
  }

  function finish(symbol, message, color) {
    if (stopped) return api;
    stopped = true;
    if (timer) {
      clearInterval(timer);
      timer = null;
    }
    clearLine();
    const c = useColor ? color : "";
    const reset = useColor ? "\x1b[39m" : "";
    stream.write(`  ${c}${symbol}${reset} ${String(message)}\n`);
    return api;
  }

  const api = {
    start() {
      if (stopped || timer) return api;
      if (isTTY) {
        render();
        timer = setInterval(render, interval);
      }
      return api;
    },
    update(newText) {
      if (stopped) return api;
      currentText = String(newText);
      render();
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

export async function withSpinner(text, fn, options = {}) {
  const task = spinner(text, options).start();
  try {
    const result = await fn(task);
    task.succeed(options.successText || text);
    return result;
  } catch (err) {
    task.fail(options.failText || err.message || text);
    throw err;
  }
}
