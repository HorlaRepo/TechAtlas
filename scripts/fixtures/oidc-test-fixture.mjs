import { createServer } from "node:http";
import { createSign, generateKeyPairSync, randomUUID } from "node:crypto";

const issuer = process.env.OIDC_TEST_ISSUER;
const audience = process.env.OIDC_TEST_AUDIENCE;
const port = Number.parseInt(process.env.OIDC_TEST_PORT ?? "8080", 10);

if (!issuer || !audience || !Number.isInteger(port)) {
  throw new Error("OIDC_TEST_ISSUER, OIDC_TEST_AUDIENCE, and OIDC_TEST_PORT must be configured.");
}

const { privateKey, publicKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });
const kid = randomUUID();
const jwk = publicKey.export({ format: "jwk" });

function issueOperatorToken() {
  const now = Math.floor(Date.now() / 1000);
  const header = Buffer.from(JSON.stringify({ alg: "RS256", kid, typ: "JWT" })).toString("base64url");
  const claims = Buffer.from(
    JSON.stringify({
      aud: audience,
      exp: now + 300,
      iat: now,
      iss: issuer,
      permissions: ["admin:operate"],
      sub: "e2e-operator",
    }),
  ).toString("base64url");
  const signingInput = `${header}.${claims}`;
  const signer = createSign("RSA-SHA256");
  signer.update(signingInput);
  signer.end();
  return `${signingInput}.${signer.sign(privateKey).toString("base64url")}`;
}

createServer((request, response) => {
  if (request.method === "GET" && request.url === "/jwks.json") {
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify({ keys: [{ ...jwk, alg: "RS256", kid, use: "sig" }] }));
    return;
  }
  if (request.method === "POST" && request.url === "/token") {
    response.writeHead(200, { "Content-Type": "application/json" });
    response.end(JSON.stringify({ access_token: issueOperatorToken(), token_type: "Bearer" }));
    return;
  }
  response.writeHead(404);
  response.end();
}).listen(port, "0.0.0.0");
