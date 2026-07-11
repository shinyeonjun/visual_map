import { CircleAlert, CircleCheck, Clock3 } from "lucide-react";
import type { EngineAvailability, EngineRegistry, EngineRole } from "../../types/engine";

type EngineState = "ok" | "pending" | "missing" | "error";

function engineForRole(registry: EngineRegistry | null, role: EngineRole): EngineAvailability | null {
  return registry?.engines.find((engine) => engine.role === role) ?? null;
}

function engineState(registry: EngineRegistry | null, engine: EngineAvailability | null, error: string | null): EngineState {
  if (error) {
    return "error";
  }
  if (!registry) {
    return "pending";
  }
  return engine?.available ? "ok" : "missing";
}

function engineText(state: EngineState, missingText?: string): string {
  if (state === "ok") {
    return "정상";
  }
  if (state === "missing") {
    return missingText ?? "설치 필요";
  }
  return state === "error" ? "오류" : "확인 전";
}

function engineTitle(registry: EngineRegistry | null, engine: EngineAvailability | null, error: string | null, missingTitle?: string): string {
  if (error) {
    return error;
  }
  if (!registry) {
    return "데스크톱 앱에서 읽기 도구 상태를 확인합니다";
  }
  if (engine?.integrity === "development") {
    return `개발용 엔진 · 배포 불가 · ${engine.path}`;
  }
  if (engine?.integrity === "development-internal") {
    return `내부 전용 개발 엔진 · 재배포 불가 · ${engine.path}`;
  }
  if (engine?.integrity === "unpublished") {
    return `공식 배포 대기 엔진 · 배포 불가 · ${engine.path}`;
  }
  if (engine?.error) {
    return `${engine.error} · ${engine.path}`;
  }
  return engine?.path ?? missingTitle ?? "읽기 도구가 필요합니다";
}

export function EngineStatus({
  label,
  role,
  registry,
  error,
  missingText,
  missingTitle,
}: {
  label: string;
  role: EngineRole;
  registry: EngineRegistry | null;
  error: string | null;
  missingText?: string;
  missingTitle?: string;
}) {
  const engine = engineForRole(registry, role);
  const state = engineState(registry, engine, error);
  const displayState = state === "missing" && missingText ? "snapshot" : state;
  const StatusIcon = state === "ok" ? CircleCheck : state === "pending" ? Clock3 : CircleAlert;
  const integrityLabel = engine?.integrity === "development"
    ? "개발용"
    : engine?.integrity === "development-internal"
      ? "내부전용"
    : engine?.integrity === "unpublished"
      ? "배포대기"
      : null;

  return (
    <span className={`engine-status ${displayState}`} title={engineTitle(registry, engine, error, missingTitle)}>
      <StatusIcon size={12} />
      {label}
      <b className={displayState}>
        {integrityLabel ?? engineText(state, missingText)}
      </b>
    </span>
  );
}

export function EngineMiniStatus({
  label,
  role,
  registry,
  error,
}: {
  label: string;
  role: EngineRole;
  registry: EngineRegistry | null;
  error: string | null;
}) {
  const engine = engineForRole(registry, role);
  const state = engineState(registry, engine, error);

  return (
    <span className={`engine-mini ${state}`} title={engineTitle(registry, engine, error)}>
      {label}
    </span>
  );
}
