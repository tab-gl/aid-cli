import crypto from "node:crypto";

export function generateVerifier() {
  return crypto.randomBytes(32).toString("base64url");
}

export function challengeFor(verifier) {
  return crypto
    .createHash("sha256")
    .update(verifier)
    .digest("base64url");
}

export function generateState() {
  return crypto.randomBytes(16).toString("base64url");
}
