import { useQuery, useQueryClient } from '@tanstack/react-query';
import { configApi } from '@/lib/api';
import type { SystemReadinessResponse } from 'shared/types';

const SYSTEM_READINESS_KEY = ['system-readiness'];
const POLL_INTERVAL_WHEN_NOT_READY = 5000; // 5 seconds

export interface UseSystemReadinessResult {
  /** The system readiness data */
  data: SystemReadinessResponse | undefined;
  /** Whether the system is ready (Claude Code installed and agent service available) */
  isReady: boolean;
  /** Whether the initial check is still loading */
  isLoading: boolean;
  /** Whether there was an error checking system readiness */
  isError: boolean;
  /** Error message if any */
  error: Error | null;
  /** Manually trigger a re-check */
  refetch: () => void;
}

/**
 * Hook to check if the system is ready to run coding agents.
 * Polls every 5 seconds if the system is not ready.
 */
export function useSystemReadiness(): UseSystemReadinessResult {
  const queryClient = useQueryClient();

  const { data, isLoading, isError, error, refetch } = useQuery({
    queryKey: SYSTEM_READINESS_KEY,
    queryFn: configApi.checkSystemReadiness,
    // Poll every 5 seconds if not ready
    refetchInterval: (query) => {
      const readinessData = query.state.data;
      if (!readinessData?.ready) {
        return POLL_INTERVAL_WHEN_NOT_READY;
      }
      return false; // Stop polling when ready
    },
    // Don't refetch on window focus when ready
    refetchOnWindowFocus: (query) => {
      const readinessData = query.state.data;
      return !readinessData?.ready;
    },
    retry: 2,
    staleTime: 1000, // 1 second
  });

  return {
    data,
    isReady: data?.ready ?? false,
    isLoading,
    isError,
    error: error as Error | null,
    refetch: () => {
      queryClient.invalidateQueries({ queryKey: SYSTEM_READINESS_KEY });
      refetch();
    },
  };
}
