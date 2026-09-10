export function reasonOf(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

export function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null;
}

export function stringField(value: unknown, field: string): string | undefined {
  if (!isRecord(value)) return undefined;
  const fieldValue = value[field];
  return typeof fieldValue === "string" && fieldValue.length > 0 ? fieldValue : undefined;
}

export function planSummary(plan: string): string | undefined {
  return plan
    .split("\n")
    .map((line) => line.trim())
    .find((line) => line.length > 0 && !line.startsWith("#"));
}

export function section(root: Record<string, unknown>, name: string): Record<string, unknown> {
  const value = root[name];
  return isRecord(value) ? value : {};
}

export function milliseconds(root: Record<string, unknown>, name: string, fallback: number): number {
  const value = root[name];
  return typeof value === "number" && Number.isFinite(value) ? value : fallback;
}
