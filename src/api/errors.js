const MAX_ERROR_BYTES = 4096;
const MAX_ERROR_CHARS = 240;

const STATUS_LABELS = {
  400: "Bad request",
  401: "Unauthorized",
  403: "Forbidden",
  404: "Not found",
  408: "Request timeout",
  409: "Conflict",
  413: "Payload too large",
  422: "Validation failed",
  429: "Rate limited"
};

export async function readApiError(response) {
  const statusLabel = STATUS_LABELS[response.status] || "HTTP error";
  let body = "";

  try {
    body = await readBoundedBody(response);
  } catch (err) {
    if (err.name === "AbortError" || err.name === "TimeoutError") {
      return `${statusLabel}; error details timed out`;
    }
    return statusLabel;
  }

  if (!body.trim()) return statusLabel;

  let detail = body;
  const contentType = response.headers.get("content-type") || "";
  if (contentType.includes("json")) {
    try {
      const parsed = JSON.parse(body);
      if (parsed && typeof parsed === "object") {
        detail = [parsed.message, parsed.error_description, parsed.detail, parsed.error]
          .find((value) => typeof value === "string" && value.trim()) || "";
      } else {
        detail = "";
      }
    } catch {
      detail = "";
    }
  } else if (/^\s*(?:<!doctype html|<html|<body|<head)/i.test(body)) {
    detail = "";
  }

  detail = sanitizeDetail(detail);
  return detail ? `${statusLabel}: ${detail}` : statusLabel;
}

async function readBoundedBody(response) {
  if (!response.body) return "";
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let bytes = 0;
  let output = "";

  try {
    while (bytes < MAX_ERROR_BYTES) {
      const { done, value } = await reader.read();
      if (done) break;
      const remaining = MAX_ERROR_BYTES - bytes;
      const chunk = value.byteLength > remaining ? value.subarray(0, remaining) : value;
      bytes += chunk.byteLength;
      output += decoder.decode(chunk, { stream: true });
      if (chunk.byteLength < value.byteLength) {
        await reader.cancel().catch(() => {});
        break;
      }
    }
    output += decoder.decode();
  } finally {
    reader.releaseLock();
  }

  return output;
}

function sanitizeDetail(value) {
  if (typeof value !== "string") return "";
  return value
    .replace(/\bBearer\s+[^\s"'<>]+/gi, "Bearer [REDACTED]")
    .replace(/\b(access_token|refresh_token|id_token|client_secret|authorization|api[_-]?key|password)\b\s*["']?\s*[:=]\s*["']?[^\s"',}]+/gi, "$1=[REDACTED]")
    .replace(/https?:\/\/[^\s"'<>]+/gi, "[URL]")
    .replace(/[\r\n\t]+/g, " ")
    .replace(/\s{2,}/g, " ")
    .trim()
    .slice(0, MAX_ERROR_CHARS);
}
