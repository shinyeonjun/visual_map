export type OperationPhase = "idle" | "running" | "success" | "error";

export type OperationStatus = {
  phase: OperationPhase;
  label: string;
  message: string;
  details?: string | null;
};

export type UserError = {
  message: string;
  details: string;
};
