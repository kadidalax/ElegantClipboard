type StringRecord = Record<string, unknown>;

export function mergeDeep<T extends StringRecord>(base: T, extra: StringRecord): T {
  const result = { ...base } as StringRecord;
  for (const key of Object.keys(extra)) {
    const baseVal = result[key];
    const extraVal = extra[key];
    if (
      baseVal &&
      extraVal &&
      typeof baseVal === "object" &&
      typeof extraVal === "object" &&
      !Array.isArray(baseVal) &&
      !Array.isArray(extraVal)
    ) {
      result[key] = mergeDeep(baseVal as StringRecord, extraVal as StringRecord);
    } else if (extraVal !== undefined) {
      result[key] = extraVal;
    }
  }
  return result as T;
}
