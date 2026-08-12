import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createHash } from "node:crypto";

const rootPackage = JSON.parse(readFileSync(new URL("../package.json", import.meta.url), "utf8"));
const pnpmLock = readFileSync(new URL("../pnpm-lock.yaml", import.meta.url), "utf8");
const cargoLock = readFileSync(new URL("../src-tauri/Cargo.lock", import.meta.url), "utf8");

const packages = new Map();
const add = (ecosystem, name, version) => {
  if (!name || !version) return;
  const key = `${ecosystem}:${name}@${version}`;
  packages.set(key, {
    SPDXID: `SPDXRef-${createHash("sha256").update(key).digest("hex").slice(0, 20)}`,
    name,
    versionInfo: version,
    downloadLocation: "NOASSERTION",
    filesAnalyzed: false,
    licenseConcluded: "NOASSERTION",
    licenseDeclared: "NOASSERTION",
    externalRefs: [{ referenceCategory: "PACKAGE-MANAGER", referenceType: "purl", referenceLocator: `pkg:${ecosystem}/${encodeURIComponent(name)}@${version}` }]
  });
};

for (const match of pnpmLock.matchAll(/^\s{2}(['"]?)(@?[^\s:'"]+(?:\/[^\s:'"]+)?)\1@([^:\s(]+).*:$/gm)) {
  add("npm", match[2], match[3]);
}

let cargoName;
for (const line of cargoLock.split("\n")) {
  const name = line.match(/^name = "([^"]+)"$/)?.[1];
  const version = line.match(/^version = "([^"]+)"$/)?.[1];
  if (name) cargoName = name;
  if (version && cargoName) {
    add("cargo", cargoName, version);
    cargoName = undefined;
  }
}

const created = new Date().toISOString();
const documentNamespace = `https://prospectingworkbench.org/spdx/${rootPackage.version}/${createHash("sha256").update(pnpmLock).update(cargoLock).digest("hex").slice(0, 24)}`;
const sbom = {
  spdxVersion: "SPDX-2.3",
  dataLicense: "CC0-1.0",
  SPDXID: "SPDXRef-DOCUMENT",
  name: `prospecting-workbench-${rootPackage.version}`,
  documentNamespace,
  creationInfo: { created, creators: ["Tool: scripts/generate-sbom.mjs"] },
  packages: [
    {
      SPDXID: "SPDXRef-RootPackage",
      name: rootPackage.name,
      versionInfo: rootPackage.version,
      downloadLocation: "NOASSERTION",
      filesAnalyzed: false,
      licenseConcluded: "Apache-2.0",
      licenseDeclared: "Apache-2.0"
    },
    ...[...packages.values()].sort((a, b) => a.name.localeCompare(b.name) || a.versionInfo.localeCompare(b.versionInfo))
  ],
  relationships: [...packages.values()].map((item) => ({ spdxElementId: "SPDXRef-RootPackage", relationshipType: "DEPENDS_ON", relatedSpdxElement: item.SPDXID }))
};

mkdirSync(new URL("../artifacts/", import.meta.url), { recursive: true });
writeFileSync(new URL("../artifacts/sbom.spdx.json", import.meta.url), `${JSON.stringify(sbom, null, 2)}\n`);
console.log(`Wrote SPDX 2.3 SBOM with ${sbom.packages.length} packages to artifacts/sbom.spdx.json`);
