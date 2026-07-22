const RESERVED_DB_IDENTITY_CHARACTERS = /[%:.]/g;

export function encodeDbIdentityComponent(value: string): string {
  return value.replace(RESERVED_DB_IDENTITY_CHARACTERS, (character) => {
    if (character === "%") return "%25";
    if (character === ":") return "%3A";
    return "%2E";
  });
}

export function decodeDbIdentityComponent(value: string): string | null {
  let decoded = "";
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (character !== "%") {
      decoded += character;
      continue;
    }
    const escape = value.slice(index, index + 3).toUpperCase();
    if (escape === "%25") decoded += "%";
    else if (escape === "%2E") decoded += ".";
    else if (escape === "%3A") decoded += ":";
    else return null;
    index += 2;
  }
  return decoded;
}

export function dbTableIdentityKey(schema: string | null | undefined, name: string): string {
  const encodedName = encodeDbIdentityComponent(name);
  return schema ? `${encodeDbIdentityComponent(schema)}.${encodedName}` : encodedName;
}

export function dbTableIdentityLabel(tableKey: string): string {
  const decoded = tableKey.split(".").map(decodeDbIdentityComponent);
  return decoded.some((part) => part === null) ? tableKey : decoded.join(".");
}

export function dbTableNameFromIdentityKey(tableKey: string): string {
  const parts = tableKey.split(".");
  const encodedName = parts[parts.length - 1] ?? tableKey;
  return decodeDbIdentityComponent(encodedName) ?? encodedName;
}

export function dbTableNodeId(tableKey: string): string {
  return `db:table:${tableKey}`;
}

export function dbColumnNodeId(tableKey: string, columnName: string): string {
  return `db:column:${tableKey}:${encodeDbIdentityComponent(columnName)}`;
}

const DB_OBJECT_KINDS = new Set([
  "database",
  "schema",
  "table",
  "column",
  "primary_key",
  "foreign_key",
  "unique_constraint",
  "check_constraint",
  "index",
  "view",
  "trigger",
  "routine",
]);

export function parseDbStableObjectKey(
  value: string | null | undefined,
): { database: string; schema: string; kind: string; objectName: string; subObject: string | null } | null {
  if (!value) return null;
  const versioned = value.startsWith("v2:");
  const rawParts = (versioned ? value.slice(3) : value).split(":");
  if ((rawParts.length !== 6 && rawParts.length !== 7) || rawParts.some((part) => !part)) return null;
  const parts = versioned ? rawParts.map(decodeDbStableKeyComponent) : rawParts;
  if (parts.some((part) => part === null)) return null;
  const decoded = parts as string[];
  if (!DB_OBJECT_KINDS.has(decoded[4])) return null;
  return {
    database: decoded[2],
    schema: decoded[3],
    kind: decoded[4],
    objectName: decoded[5],
    subObject: decoded[6] ?? null,
  };
}

function decodeDbStableKeyComponent(value: string): string | null {
  let decoded = "";
  for (let index = 0; index < value.length; index += 1) {
    const character = value[index];
    if (character !== "%") {
      decoded += character;
      continue;
    }
    const escape = value.slice(index, index + 3).toUpperCase();
    if (escape === "%25") decoded += "%";
    else if (escape === "%3A") decoded += ":";
    else return null;
    index += 2;
  }
  return decoded;
}
