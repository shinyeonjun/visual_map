import { collectSearchResults } from "./search";
import type { CodeInventory, DbInventory } from "../types/workspace";

type SearchRequest = {
  id: number;
  query: string;
  codeInventory: CodeInventory | null;
  dbInventory: DbInventory | null;
};

type SearchResponse = {
  id: number;
  result?: ReturnType<typeof collectSearchResults>;
  error?: string;
};

const workerScope = self as unknown as {
  onmessage: ((event: MessageEvent<SearchRequest>) => void) | null;
  postMessage: (message: SearchResponse) => void;
};

workerScope.onmessage = ({ data }: MessageEvent<SearchRequest>) => {
  try {
    workerScope.postMessage({
      id: data.id,
      result: collectSearchResults(data.query, data.codeInventory, data.dbInventory),
    });
  } catch (error) {
    workerScope.postMessage({ id: data.id, error: error instanceof Error ? error.message : String(error) });
  }
};
