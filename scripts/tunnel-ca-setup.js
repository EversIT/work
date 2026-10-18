#!/usr/bin/env node

/**
 * One-time operator setup for the self-hosted tunnel's mTLS trust chain (see
 * apps/tunnel-server, apps/tunnel-client, apps/backend/src/app/tunnels).
 *
 * Generates a private root CA and a server certificate for tunnel-server's
 * control-plane TLS listener, both signed with the CA. The backend uses the
 * CA to sign a fresh client certificate for every tunnel it creates - this
 * script only needs to run once per deployment (or whenever you want to
 * rotate the CA itself, which invalidates every previously-issued client
 * certificate - existing tunnels would need their tokens regenerated to get
 * a new one).
 *
 * Usage: node scripts/tunnel-ca-setup.js
 *
 * Prints four PEM values to store as secrets:
 *   TUNNEL_CA_CERT           -> backend, tunnel-server, tunnel-client
 *   TUNNEL_CA_KEY             -> backend only (signs new client certs)
 *   TUNNEL_CONTROL_TLS_CERT  -> tunnel-server only
 *   TUNNEL_CONTROL_TLS_KEY   -> tunnel-server only
 */

const { webcrypto } = require('crypto');
const readline = require('readline');
const x509 = require('@peculiar/x509');

x509.cryptoProvider.set(webcrypto);

const SIGNING_ALGORITHM = {
  name: 'ECDSA',
  namedCurve: 'P-256',
  hash: 'SHA-256',
};
const CA_VALIDITY_YEARS = 10;
const SERVER_CERT_VALIDITY_YEARS = 5;

async function generateKeyPair() {
  return webcrypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-256' },
    true,
    ['sign', 'verify'],
  );
}

async function toPkcs8Pem(privateKey) {
  const pkcs8 = await webcrypto.subtle.exportKey('pkcs8', privateKey);
  return x509.PemConverter.encode(pkcs8, 'PRIVATE KEY');
}

function notAfterYears(years) {
  const date = new Date();
  date.setFullYear(date.getFullYear() + years);
  return date;
}

async function question(prompt) {
  const rl = readline.createInterface({
    input: process.stdin,
    output: process.stdout,
  });
  try {
    return await new Promise((resolve) => rl.question(prompt, resolve));
  } finally {
    rl.close();
  }
}

async function main() {
  const controlHostname = (
    await question(
      'Control-plane hostname clients will connect to (e.g. tunnel.example.com): ',
    )
  ).trim();
  if (!controlHostname) {
    console.error(
      "A hostname is required (used as the server certificate's CN/SAN).",
    );
    process.exit(1);
  }

  const caKeys = await generateKeyPair();
  const caCert = await x509.X509CertificateGenerator.createSelfSigned({
    serialNumber: '01',
    name: 'CN=Eversit Tunnel CA',
    notBefore: new Date(),
    notAfter: notAfterYears(CA_VALIDITY_YEARS),
    signingAlgorithm: SIGNING_ALGORITHM,
    keys: caKeys,
    extensions: [
      new x509.BasicConstraintsExtension(true, undefined, true),
      new x509.KeyUsagesExtension(
        x509.KeyUsageFlags.keyCertSign | x509.KeyUsageFlags.cRLSign,
        true,
      ),
    ],
  });

  const serverKeys = await generateKeyPair();
  const serverCert = await x509.X509CertificateGenerator.create({
    serialNumber: '02',
    subject: `CN=${controlHostname}`,
    issuer: caCert.subject,
    notBefore: new Date(),
    notAfter: notAfterYears(SERVER_CERT_VALIDITY_YEARS),
    signingAlgorithm: SIGNING_ALGORITHM,
    publicKey: serverKeys.publicKey,
    signingKey: caKeys.privateKey,
    extensions: [
      new x509.KeyUsagesExtension(
        x509.KeyUsageFlags.digitalSignature |
          x509.KeyUsageFlags.keyEncipherment,
        true,
      ),
      new x509.ExtendedKeyUsageExtension(['1.3.6.1.5.5.7.3.1'], true), // serverAuth
      new x509.SubjectAlternativeNameExtension([
        { type: 'dns', value: controlHostname },
      ]),
    ],
  });

  const caCertPem = caCert.toString('pem');
  const caKeyPem = await toPkcs8Pem(caKeys.privateKey);
  const serverCertPem = serverCert.toString('pem');
  const serverKeyPem = await toPkcs8Pem(serverKeys.privateKey);

  console.log(
    '\nStore these as secrets - none of them are printed anywhere else, and this script keeps no state.\n',
  );
  console.log('# backend, tunnel-server, tunnel-client');
  console.log(`TUNNEL_CA_CERT=${toEnvValue(caCertPem)}\n`);
  console.log(
    "# backend only - signs every new tunnel's client certificate, keep this the most tightly guarded of the four",
  );
  console.log(`TUNNEL_CA_KEY=${toEnvValue(caKeyPem)}\n`);
  console.log(
    '# tunnel-server only - its own identity for the control-plane TLS listener',
  );
  console.log(`TUNNEL_CONTROL_TLS_CERT=${toEnvValue(serverCertPem)}\n`);
  console.log(`TUNNEL_CONTROL_TLS_KEY=${toEnvValue(serverKeyPem)}\n`);
}

// Env vars don't reliably carry embedded newlines across every shell/Docker
// invocation - base64-encode multi-line PEM so it survives as a single
// token; every consumer (backend, tunnel-server, tunnel-client) decodes it
// back before use.
function toEnvValue(pem) {
  return Buffer.from(pem, 'utf8').toString('base64');
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
