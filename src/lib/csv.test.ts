import { describe, expect, it } from "vitest";
import { parseSeedInput } from "./csv";

describe("parseSeedInput", () => {
  it("accepts newline-separated domains", () => {
    expect(parseSeedInput("example.com\nhttps://acme.test/about")).toEqual([
      { companyName: "example.com", domain: "example.com", websiteUrl: "https://example.com" },
      { companyName: "acme.test", domain: "acme.test", websiteUrl: "https://acme.test/about" }
    ]);
  });

  it("maps common CSV aliases without guessing omitted contacts", () => {
    expect(
      parseSeedInput(
        'Company Name,Website,Person,Job Title,Work Email,Business Phone\n"Acme, Inc",https://acme.test,Ada Lovelace,CTO,ada@acme.test,+1 555 555 1212'
      )
    ).toEqual([
      {
        companyName: "Acme, Inc",
        websiteUrl: "https://acme.test",
        personName: "Ada Lovelace",
        personTitle: "CTO",
        workEmail: "ada@acme.test",
        businessPhone: "+1 555 555 1212"
      }
    ]);
  });
});
