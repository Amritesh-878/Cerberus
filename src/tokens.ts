import { invoke } from "@tauri-apps/api/core";

import type { AccountId } from "./accounts";

export interface Tokens {
  input: number;
  output: number;
  cacheRead: number;
  cacheWrite: number;
}

/** One model's tokens over a range, and their cost at API prices; `null` when the model has no price. */
export interface ModelRow {
  model: string;
  tokens: Tokens;
  cost: number | null;
}

export interface AccountTokens {
  account: AccountId;
  today: ModelRow[];
  week: ModelRow[];
  month: ModelRow[];
}

export type Range = "today" | "week" | "month";

export const RANGES: { id: Range; label: string }[] = [
  { id: "today", label: "Today" },
  { id: "week", label: "7 days" },
  { id: "month", label: "30 days" },
];

/** Reads what changed in the chat files; the first call reads them all, so it can take a few seconds. */
export function usageTokens(): Promise<AccountTokens[]> {
  return invoke<AccountTokens[]>("usage_tokens", {
    offsetMinutes: -new Date().getTimezoneOffset(),
  });
}

const COMPACT = new Intl.NumberFormat("en-US", { notation: "compact" });

/** "950", "34K", "1.2M"; rounding rolls over, so 999,600 reads "1M". */
export function countText(count: number): string {
  return COMPACT.format(count);
}

/** "$3.10", "<$0.01", or "no price". */
export function costText(cost: number | null): string {
  if (cost === null) return "no price";
  if (cost > 0 && cost < 0.01) return "<$0.01";
  return `$${cost.toFixed(2)}`;
}

export function tokensText(tokens: Tokens): string {
  return [
    `${countText(tokens.input)} in`,
    `${countText(tokens.output)} out`,
    `${countText(tokens.cacheRead)} cache read`,
    `${countText(tokens.cacheWrite)} cache write`,
  ].join(" · ");
}

/** An account's total over its rows; models with no price count as nothing. */
export function totalCost(rows: ModelRow[]): number {
  return rows.reduce((sum, row) => sum + (row.cost ?? 0), 0);
}
