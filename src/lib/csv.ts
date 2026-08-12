import type { AccountSeed } from "../types";

const FIELD_ALIASES: Record<string, keyof AccountSeed> = {
  company: "companyName",
  company_name: "companyName",
  organization: "companyName",
  domain: "domain",
  website: "websiteUrl",
  website_url: "websiteUrl",
  url: "websiteUrl",
  name: "personName",
  person: "personName",
  person_name: "personName",
  title: "personTitle",
  job_title: "personTitle",
  email: "workEmail",
  work_email: "workEmail",
  phone: "businessPhone",
  business_phone: "businessPhone"
};

function splitCsvLine(line: string): string[] {
  const values: string[] = [];
  let current = "";
  let quoted = false;

  for (let index = 0; index < line.length; index += 1) {
    const char = line[index];
    if (char === '"' && line[index + 1] === '"' && quoted) {
      current += '"';
      index += 1;
    } else if (char === '"') {
      quoted = !quoted;
    } else if (char === "," && !quoted) {
      values.push(current.trim());
      current = "";
    } else {
      current += char;
    }
  }
  values.push(current.trim());
  return values;
}
export function parseSeedInput(input: string): AccountSeed[] {
  const lines = input
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean);
  if (!lines.length) return [];

  const first = splitCsvLine(lines[0]);
  const mappedHeaders = first.map(
    (value) => FIELD_ALIASES[value.toLowerCase().replace(/[\s-]+/g, "_")]
  );
  const hasHeaders = mappedHeaders.some(Boolean);

  if (hasHeaders) {
    return lines.slice(1).flatMap((line) => {
      const values = splitCsvLine(line);
      const seed: Partial<AccountSeed> = {};
      mappedHeaders.forEach((field, index) => {
        if (field && values[index]) seed[field] = values[index];
      });
      if (!seed.companyName) seed.companyName = seed.domain ?? seed.websiteUrl ?? "";
      return seed.companyName ? [seed as AccountSeed] : [];
    });
  }

  return lines.map((line) => {
    const looksLikeUrl = /^https?:\/\//i.test(line);
    const value = line.replace(/^https?:\/\//i, "").replace(/^www\./i, "").split("/")[0];
    return {
      companyName: value,
      domain: value,
      websiteUrl: looksLikeUrl ? line : `https://${value}`
    };
  });
}
