export type EngineRole = "code" | "db";

export type EngineAvailability = {
  id: string;
  label: string;
  role: EngineRole;
  executable: string;
  expectedVersion: string;
  contractVersion: string;
  path: string;
  available: boolean;
  releasable: boolean;
  integrity: "release" | "development" | "development-internal" | "unpublished" | "unpublished-internal" | "missing" | "mismatch" | "manifest-error" | string;
  sha256?: string | null;
  error?: string | null;
};

export type EngineRegistry = {
  mode: "dev" | "internal" | "production";
  engineDir: string;
  engines: EngineAvailability[];
};
