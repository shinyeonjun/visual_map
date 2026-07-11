import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import { toUserError } from "../app/operationStatus";
import { hasTauriRuntime } from "../app/tauriRuntime";
import type { EngineRegistry } from "../types/engine";

export function useEngineRegistry() {
  const [engineRegistry, setEngineRegistry] = useState<EngineRegistry | null>(null);
  const [engineError, setEngineError] = useState<string | null>(null);

  useEffect(() => {
    if (!hasTauriRuntime()) {
      return;
    }

    invoke<EngineRegistry>("get_engine_availability")
      .then((registry) => {
        setEngineRegistry(registry);
        setEngineError(null);
      })
      .catch((error: unknown) => {
        setEngineRegistry(null);
        setEngineError(toUserError(error, "읽기 도구 상태를 확인하지 못했습니다").message);
      });
  }, []);

  return { engineRegistry, engineError };
}
