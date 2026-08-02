import { useCallback, useState } from 'react';
import type { SearchResult } from '@aurora/shared-types';

export interface UseAIResult {
  complete: (prompt: string) => Promise<string>;
  summarize: (content: string) => Promise<string>;
  search: (query: string) => Promise<SearchResult[]>;
  isGenerating: boolean;
}

const MOCK_DELAY_MS = 50;

function mockResult<T>(value: T, onDone: () => void): Promise<T> {
  return new Promise<T>((resolve) => {
    setTimeout(() => {
      onDone();
      resolve(value);
    }, MOCK_DELAY_MS);
  });
}

/**
 * Mock AI hook. All calls resolve via `setTimeout`; `isGenerating` is true
 * while any call is in flight. A real implementation would delegate to an
 * `AIProvider`.
 */
export function useAI(): UseAIResult {
  const [isGenerating, setIsGenerating] = useState(false);

  const start = useCallback((): void => setIsGenerating(true), []);
  const stop = useCallback((): void => setIsGenerating(false), []);

  const complete = useCallback(
    (prompt: string): Promise<string> => {
      start();
      return mockResult(`[mock completion for: ${prompt}]`, stop);
    },
    [start, stop],
  );

  const summarize = useCallback(
    (content: string): Promise<string> => {
      start();
      return mockResult(`[mock summary of ${content.length} chars]`, stop);
    },
    [start, stop],
  );

  const search = useCallback(
    (query: string): Promise<SearchResult[]> => {
      start();
      const results: SearchResult[] = [
        {
          doc_id: 'mock-doc-1',
          title: `Result for "${query}"`,
          score: 1,
          snippet: null,
          matched_by: 'hybrid',
        },
      ];
      return mockResult(results, stop);
    },
    [start, stop],
  );

  return { complete, summarize, search, isGenerating };
}
