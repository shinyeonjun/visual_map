import type { CodeInventory, DbInventory } from "../types/workspace";
import { collectSearchResults, SEARCH_WORKER_THRESHOLD, searchInventorySize, type SearchCollection } from "./search";

// ponytail: keep small searches on the main thread; worker startup and structured cloning cost more than the scan below this ceiling.

type PendingSearch = {
  resolve: (collection: SearchCollection) => void;
  reject: (error: Error) => void;
};

let worker: Worker | null = null;
let nextRequestId = 0;
const pending = new Map<number, PendingSearch>();

export function collectSearchResultsAsync(
  query: string,
  codeInventory: CodeInventory | null,
  dbInventory: DbInventory | null,
): Promise<SearchCollection> {
  if (searchInventorySize(codeInventory, dbInventory) < SEARCH_WORKER_THRESHOLD || typeof Worker === "undefined") {
    return Promise.resolve(collectSearchResults(query, codeInventory, dbInventory));
  }

  let searchWorker: Worker;
  try {
    searchWorker = getWorker();
    const id = ++nextRequestId;
    return new Promise<SearchCollection>((resolve, reject) => {
      pending.set(id, { resolve, reject });
      try {
        searchWorker.postMessage({ id, query, codeInventory, dbInventory });
      } catch {
        pending.delete(id);
        resolve(collectSearchResults(query, codeInventory, dbInventory));
      }
    }).catch(() => collectSearchResults(query, codeInventory, dbInventory));
  } catch {
    return Promise.resolve(collectSearchResults(query, codeInventory, dbInventory));
  }
}

function getWorker(): Worker {
  if (worker) {
    return worker;
  }
  worker = new Worker(new URL("./search.worker.ts", import.meta.url), { type: "module" });
  worker.onmessage = ({ data }: MessageEvent<{ id: number; result?: SearchCollection; error?: string }>) => {
    const request = pending.get(data.id);
    if (!request) {
      return;
    }
    pending.delete(data.id);
    if (data.result) {
      request.resolve(data.result);
    } else {
      request.reject(new Error(data.error ?? "검색 worker가 결과를 반환하지 않았습니다"));
    }
  };
  worker.onerror = (event) => {
    const error = new Error(event.message || "검색 worker를 실행하지 못했습니다");
    for (const request of pending.values()) {
      request.reject(error);
    }
    pending.clear();
    worker?.terminate();
    worker = null;
  };
  return worker;
}
