import { useEffect, useState, useRef, useCallback } from 'react';
import type {
  AgentSession,
  AgentMessage,
  AgentPendingInput,
  AgentState,
} from 'shared/types';

interface AgentSessionEvent {
  type: 'state_change' | 'message' | 'pending_input' | 'error';
  data: unknown;
}

interface UseAgentSessionResult {
  session: AgentSession | null;
  messages: AgentMessage[];
  pendingInputs: AgentPendingInput[];
  isConnected: boolean;
  error: string | null;
  sendQuery: (query: string) => Promise<void>;
  respondToQuestion: (inputId: string, response: string) => Promise<void>;
  approveToolUse: (inputId: string, approved: boolean) => Promise<void>;
  pauseSession: () => Promise<void>;
  resumeSession: () => Promise<void>;
  terminateSession: () => Promise<void>;
}

export const useAgentSession = (
  agentSessionId: string | undefined
): UseAgentSessionResult => {
  const [session, setSession] = useState<AgentSession | null>(null);
  const [messages, setMessages] = useState<AgentMessage[]>([]);
  const [pendingInputs, setPendingInputs] = useState<AgentPendingInput[]>([]);
  const [isConnected, setIsConnected] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const wsRef = useRef<WebSocket | null>(null);
  const retryCountRef = useRef<number>(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const isIntentionallyClosed = useRef<boolean>(false);

  // Fetch initial session data
  useEffect(() => {
    if (!agentSessionId) {
      setSession(null);
      setMessages([]);
      setPendingInputs([]);
      return;
    }

    const fetchInitialData = async () => {
      try {
        const [sessionRes, messagesRes, inputsRes] = await Promise.all([
          fetch(`/api/agents/sessions/${agentSessionId}`),
          fetch(`/api/agents/sessions/${agentSessionId}/messages`),
          fetch(`/api/agents/sessions/${agentSessionId}/pending-inputs`),
        ]);

        if (sessionRes.ok) {
          const sessionData = await sessionRes.json();
          setSession(sessionData);
        }
        if (messagesRes.ok) {
          const messagesData = await messagesRes.json();
          setMessages(messagesData);
        }
        if (inputsRes.ok) {
          const inputsData = await inputsRes.json();
          setPendingInputs(inputsData.filter((i: AgentPendingInput) => !i.responded_at));
        }
      } catch (e) {
        console.error('Failed to fetch initial agent session data:', e);
        setError('Failed to load session data');
      }
    };

    fetchInitialData();
  }, [agentSessionId]);

  // WebSocket connection for real-time events
  useEffect(() => {
    if (!agentSessionId) {
      return;
    }

    const open = () => {
      const protocol = window.location.protocol === 'https:' ? 'wss:' : 'ws:';
      const host = window.location.host;
      const ws = new WebSocket(
        `${protocol}//${host}/api/agents/sessions/${agentSessionId}/events`
      );
      wsRef.current = ws;
      isIntentionallyClosed.current = false;

      ws.onopen = () => {
        setError(null);
        setIsConnected(true);
        retryCountRef.current = 0;
      };

      ws.onmessage = (event) => {
        try {
          const msg: AgentSessionEvent = JSON.parse(event.data);

          switch (msg.type) {
            case 'state_change':
              setSession((prev) =>
                prev
                  ? { ...prev, state: msg.data as AgentState, updated_at: new Date().toISOString() }
                  : null
              );
              break;
            case 'message':
              setMessages((prev) => [...prev, msg.data as AgentMessage]);
              break;
            case 'pending_input':
              setPendingInputs((prev) => [...prev, msg.data as AgentPendingInput]);
              break;
            case 'error':
              setError(msg.data as string);
              break;
          }
        } catch (e) {
          console.error('Failed to parse WebSocket message:', e);
        }
      };

      ws.onerror = () => {
        setError('Connection failed');
      };

      ws.onclose = (event) => {
        setIsConnected(false);
        // Only retry if the close was not intentional and not a normal closure
        if (!isIntentionallyClosed.current && event.code !== 1000) {
          const next = retryCountRef.current + 1;
          retryCountRef.current = next;
          if (next <= 6) {
            const delay = Math.min(1500, 250 * 2 ** (next - 1));
            retryTimerRef.current = setTimeout(() => open(), delay);
          }
        }
      };
    };

    open();

    return () => {
      if (wsRef.current) {
        isIntentionallyClosed.current = true;
        wsRef.current.close();
        wsRef.current = null;
      }
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
    };
  }, [agentSessionId]);

  const sendQuery = useCallback(
    async (query: string) => {
      if (!agentSessionId) return;
      try {
        const res = await fetch(`/api/agents/sessions/${agentSessionId}/query`, {
          method: 'POST',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ query }),
        });
        if (!res.ok) {
          const errorData = await res.json();
          setError(errorData.message || 'Failed to send query');
        }
      } catch (e) {
        console.error('Failed to send query:', e);
        setError('Failed to send query');
      }
    },
    [agentSessionId]
  );

  const respondToQuestion = useCallback(
    async (inputId: string, response: string) => {
      if (!agentSessionId) return;
      try {
        const res = await fetch(
          `/api/agents/sessions/${agentSessionId}/respond/${inputId}`,
          {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ response }),
          }
        );
        if (res.ok) {
          setPendingInputs((prev) => prev.filter((i) => i.id !== inputId));
        } else {
          const errorData = await res.json();
          setError(errorData.message || 'Failed to respond');
        }
      } catch (e) {
        console.error('Failed to respond to question:', e);
        setError('Failed to respond');
      }
    },
    [agentSessionId]
  );

  const approveToolUse = useCallback(
    async (inputId: string, approved: boolean) => {
      if (!agentSessionId) return;
      try {
        const res = await fetch(
          `/api/agents/sessions/${agentSessionId}/approve/${inputId}`,
          {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ approved }),
          }
        );
        if (res.ok) {
          setPendingInputs((prev) => prev.filter((i) => i.id !== inputId));
        } else {
          const errorData = await res.json();
          setError(errorData.message || 'Failed to approve');
        }
      } catch (e) {
        console.error('Failed to approve tool use:', e);
        setError('Failed to approve');
      }
    },
    [agentSessionId]
  );

  const pauseSession = useCallback(async () => {
    if (!agentSessionId) return;
    try {
      const res = await fetch(`/api/agents/sessions/${agentSessionId}/pause`, {
        method: 'POST',
      });
      if (!res.ok) {
        const errorData = await res.json();
        setError(errorData.message || 'Failed to pause session');
      }
    } catch (e) {
      console.error('Failed to pause session:', e);
      setError('Failed to pause session');
    }
  }, [agentSessionId]);

  const resumeSession = useCallback(async () => {
    if (!agentSessionId) return;
    try {
      const res = await fetch(`/api/agents/sessions/${agentSessionId}/resume`, {
        method: 'POST',
      });
      if (!res.ok) {
        const errorData = await res.json();
        setError(errorData.message || 'Failed to resume session');
      }
    } catch (e) {
      console.error('Failed to resume session:', e);
      setError('Failed to resume session');
    }
  }, [agentSessionId]);

  const terminateSession = useCallback(async () => {
    if (!agentSessionId) return;
    try {
      const res = await fetch(`/api/agents/sessions/${agentSessionId}`, {
        method: 'DELETE',
      });
      if (!res.ok) {
        const errorData = await res.json();
        setError(errorData.message || 'Failed to terminate session');
      }
    } catch (e) {
      console.error('Failed to terminate session:', e);
      setError('Failed to terminate session');
    }
  }, [agentSessionId]);

  return {
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
  };
};
