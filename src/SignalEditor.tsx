import { useEffect, useState } from "react";
import { blankRule, formatKeywords, parseKeywords } from "./lib/search";
import type { QualificationRule } from "./types";

const ORIGIN_LABEL: Record<string, string> = {
  pack: "Product pack",
  catalog: "Business type",
  model: "Local model",
  query: "Your words",
  user: "Added by you"
};

/**
 * One rule's keywords. Edited as free text and parsed on blur, so typing a
 * comma or an opening quote never fights the parser mid-word.
 */
function KeywordInput({ rule, onChange }: { rule: QualificationRule; onChange: (keywords: string[]) => void }) {
  const [text, setText] = useState(formatKeywords(rule.keywords));
  useEffect(() => setText(formatKeywords(rule.keywords)), [rule.keywords.join("\u0000")]);
  return <input aria-label={`Keywords for ${rule.label || "new signal"}`} value={text} placeholder="now hiring servers, weekly pay, &quot;locations&quot;" onChange={(event) => setText(event.target.value)} onBlur={() => onChange(parseKeywords(text))} />;
}

/**
 * The plan's qualification signals, editable before anything runs.
 *
 * Each rule is a label and the phrases a good customer's own website would
 * show. A required rule must be found for a candidate to qualify. Scoring
 * applies these deterministically; nothing here is sent to a model.
 */
export function SignalEditor({ rules, status, onChange }: { rules: QualificationRule[]; status?: string; onChange: (rules: QualificationRule[]) => void }) {
  const update = (index: number, patch: Partial<QualificationRule>) => onChange(rules.map((rule, position) => position === index ? { ...rule, ...patch } : rule));
  const signals = rules.filter((rule) => rule.component === "signal").length;
  return <section className="signal-editor" aria-label="Qualification signals">
    <div className="signal-editor-heading">
      <div><span className="eyebrow">Signals a good customer's website would show</span><p>A candidate qualifies on two independent signals. Mark a signal required when every match must show it.</p></div>
      {status && <small className="signal-status" role="status">{status}</small>}
    </div>
    {signals === 0 && <p className="alert warning">With no signals, no candidate can qualify. Add at least one.</p>}
    <ul className="signal-list">
      {rules.map((rule, index) => <li key={rule.id} className="signal-row">
        <div className="signal-row-top">
          <input className="signal-label" aria-label="Signal name" value={rule.label} placeholder="Name this signal" onChange={(event) => update(index, { label: event.target.value })} />
          <span className="chip muted-chip">{ORIGIN_LABEL[rule.origin ?? ""] ?? "Plan"}</span>
          {rule.component === "icp" && <span className="chip muted-chip" title="Counts toward fit rather than buying signals">Fit</span>}
          <label className="signal-required"><input type="checkbox" checked={rule.hardGate} onChange={(event) => update(index, { hardGate: event.target.checked })} />Required</label>
          <button type="button" className="button ghost danger-text" aria-label={`Remove ${rule.label || "signal"}`} onClick={() => onChange(rules.filter((_, position) => position !== index))}>Remove</button>
        </div>
        <KeywordInput rule={rule} onChange={(keywords) => update(index, { keywords })} />
      </li>)}
    </ul>
    {rules.length < 10 && <button type="button" className="button secondary" onClick={() => onChange([...rules, blankRule(rules)])}>Add a signal</button>}
  </section>;
}
