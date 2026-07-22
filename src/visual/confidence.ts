export type LinkConfidence = "high" | "medium" | "low";

const CONFIDENCE_LABELS: Record<LinkConfidence, string> = {
  high: "단서 강함",
  medium: "단서 보통",
  low: "단서 약함",
};

const CONFIDENCE_REASONS: Record<LinkConfidence, string> = {
  high: "식별자나 경로 단서가 강하게 일치합니다. 실제 의존성이 확정된 것은 아닙니다.",
  medium: "식별자나 경로가 비슷한 후보입니다. 실제 사용 여부를 확인해야 합니다.",
  low: "약한 이름 단서만 있습니다. 실제 사용 여부를 확인해야 합니다.",
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
  if (confidence === "high" || confidence === "medium") {
    return "amber";
  }
  return "gray";
}
