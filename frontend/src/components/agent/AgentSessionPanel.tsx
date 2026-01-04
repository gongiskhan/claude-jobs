import { useState, useCallback } from 'react';
import { useAgentSessionContext } from '@/contexts/AgentSessionContext';
import { AgentSessionStatus } from './AgentSessionStatus';
import { AgentMessageList } from './AgentMessageList';
import { AgentPendingInputCard } from './AgentPendingInputCard';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Send, Loader2 } from 'lucide-react';
import { cn } from '@/lib/utils';

interface AgentSessionPanelProps {
  className?: string;
}

export function AgentSessionPanel({ className }: AgentSessionPanelProps) {
  const {
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
  } = useAgentSessionContext();

  const [queryInput, setQueryInput] = useState('');
  const [isSending, setIsSending] = useState(false);

  const canSendQuery =
    session?.state === 'idle' && queryInput.trim() && !isSending;

  const handleSendQuery = useCallback(async () => {
    if (!canSendQuery) return;
    setIsSending(true);
    try {
      await sendQuery(queryInput);
      setQueryInput('');
    } finally {
      setIsSending(false);
    }
  }, [canSendQuery, queryInput, sendQuery]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSendQuery();
      }
    },
    [handleSendQuery]
  );

  return (
    <div
      className={cn(
        'flex flex-col h-full min-h-0 bg-background border rounded-lg',
        className
      )}
    >
      {/* Status bar */}
      <AgentSessionStatus
        session={session}
        isConnected={isConnected}
        error={error}
        onPause={pauseSession}
        onResume={resumeSession}
        onTerminate={terminateSession}
      />

      {/* Message list - scrollable area */}
      <div className="flex-1 min-h-0 overflow-hidden">
        <AgentMessageList messages={messages} className="h-full" />
      </div>

      {/* Pending inputs */}
      {pendingInputs.length > 0 && (
        <div className="border-t p-4 space-y-3 max-h-[40%] overflow-y-auto">
          {pendingInputs.map((input) => (
            <AgentPendingInputCard
              key={input.id}
              input={input}
              onRespond={respondToQuestion}
              onApprove={approveToolUse}
            />
          ))}
        </div>
      )}

      {/* Query input - always visible at bottom */}
      <div className="border-t p-4">
        <div className="flex gap-2">
          <Textarea
            value={queryInput}
            onChange={(e) => setQueryInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder={
              session?.state === 'idle'
                ? 'Send a query to the agent...'
                : session?.state === 'executing'
                  ? 'Agent is working...'
                  : session?.state === 'awaiting_input'
                    ? 'Respond to the agent question above'
                    : 'Session is not active'
            }
            disabled={session?.state !== 'idle' || isSending}
            className="min-h-[60px] resize-none"
          />
          <Button
            onClick={handleSendQuery}
            disabled={!canSendQuery}
            className="self-end"
          >
            {isSending ? (
              <Loader2 className="h-4 w-4 animate-spin" />
            ) : (
              <Send className="h-4 w-4" />
            )}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground mt-1">
          Press Cmd+Enter to send
        </p>
      </div>
    </div>
  );
}
