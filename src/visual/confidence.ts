export type LinkConfidence = "high" | "medium" | "low";

const CONFIDENCE_LABELS: Record<LinkConfidence, string> = {
  high: "높음",
  medium: "보통",
  low: "낮음",
};

const CONFIDENCE_REASONS: Record<LinkConfidence, string> = {
  high: "강한 이름/구조 단서가 일치합니다.",
  medium: "이름이 비슷한 후보 단서가 있습니다.",
  low: "약한 이름 단서만 있어 확인이 필요합니다.",
};

export function normalizeConfidence(value?: string | null): LinkConfidence | null {
  if (!value) {
    return null;
  }

  const normalized = value.trim().toLowerCase();
  if (normalized === "high" || normalized === "medium" || normalized === "low") {
    return normalized;
  }

  const score = Number(normalized);
  if (!Number.isFinite(score)) {
    return null;
  }
  if (score >= 0.75) {
    return "high";
  }
  if (score >= 0.45) {
    return "medium";
  }
  return "low";
}

export function confidenceLabel(value?: string | null): string | null {
  const confidence = normalizeConfidence(value);
  return confidence ? CONFIDENCE_LABELS[confidence] : null;
}

export function confidenceReason(value?: string | null): string {
  const confidence = normalizeConfidence(value);
  return confidence ? CONFIDENCE_REASONS[confidence] : "확정 또는 후보 근거입니다.";
}

export function confidenceBadgeTone(value?: string | null): "green" | "amber" | "gray" {
  const confidence = normalizeConfidence(value);
  if (confidence === "high") {
    return "green";
  }
  if (confidence === "medium") {
    return "amber";
  }
  return "gray";
}
