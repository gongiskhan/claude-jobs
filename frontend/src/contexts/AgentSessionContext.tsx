import {
  createContext,
  useContext,
  useState,
  useMemo,
  useCallback,
  ReactNode,
} from 'react';
import type {
  AgentSession,
  AgentMessage,
  AgentPendingInput,
} from 'shared/types';
import { useAgentSession } from '@/hooks/useAgentSession';

interface AgentSessionContextType {
  agentSessionId: string | null;
  session: AgentSession | null;
  messages: AgentMessage[];
  pendingInputs: AgentPendingInput[];
  isConnected: boolean;
  error: string | null;
  setAgentSessionId: (id: string | null) => void;
  sendQuery: (query: string) => Promise<void>;
  respondToQuestion: (inputId: string, response: string) => Promise<void>;
  approveToolUse: (inputId: string, approved: boolean) => Promise<void>;
  pauseSession: () => Promise<void>;
  resumeSession: () => Promise<void>;
  terminateSession: () => Promise<void>;
  clearError: () => void;
}

const AgentSessionContext = createContext<AgentSessionContextType | null>(null);

interface AgentSessionProviderProps {
  children: ReactNode;
  initialSessionId?: string;
}

export const AgentSessionProvider = ({
  children,
  initialSessionId,
}: AgentSessionProviderProps) => {
  const [agentSessionId, setAgentSessionId] = useState<string | null>(
    initialSessionId ?? null
  );
  const [localError, setLocalError] = useState<string | null>(null);

  const {
    session,
    messages,
    pendingInputs,
    isConnected,
    error: hookError,
    sendQuery,
    respondToQuestion,
    approveToolUse,
    pauseSession,
    resumeSession,
    terminateSession,
  } = useAgentSession(agentSessionId ?? undefined);

  const error = hookError || localError;

  const clearError = useCallback(() => {
    setLocalError(null);
  }, []);

  const value = useMemo(
    () => ({
      agentSessionId,
      session,
      messages,
      pendingInputs,
      isConnected,
      error,
      setAgentSessionId,
      sendQuery,
      respondToQuestion,
      approveToolUse,
      pauseSession,
      resumeSession,
      terminateSession,
      clearError,
    }),
    [
      agentSessionId,
      session,
      messages,
      pendingInputs,
      isConnected,
      error,
      sendQuery,
      respondToQuestion,
      approveToolUse,
      pauseSession,
      resumeSession,
      terminateSession,
      clearError,
    ]
  );

  return (
    <AgentSessionContext.Provider value={value}>
      {children}
    </AgentSessionContext.Provider>
  );
};

export const useAgentSessionContext = (): AgentSessionContextType => {
  const context = useContext(AgentSessionContext);
  if (!context) {
    throw new Error(
      'useAgentSessionContext must be used within an AgentSessionProvider'
    );
  }
  return context;
};
