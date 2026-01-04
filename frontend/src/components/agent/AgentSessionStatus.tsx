import type { AgentSession } from 'shared/types';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  Loader2,
  Pause,
  Play,
  Square,
  Wifi,
  WifiOff,
  AlertCircle,
  CheckCircle,
  Clock,
  HelpCircle,
} from 'lucide-react';
import { cn } from '@/lib/utils';

interface AgentSessionStatusProps {
  session: AgentSession | null;
  isConnected: boolean;
  error: string | null;
  onPause: () => Promise<void>;
  onResume: () => Promise<void>;
  onTerminate: () => Promise<void>;
}

function StateIcon({ state }: { state: string }) {
  switch (state) {
    case 'created':
    case 'idle':
      return <Clock className="h-4 w-4" />;
    case 'executing':
      return <Loader2 className="h-4 w-4 animate-spin" />;
    case 'awaiting_input':
      return <HelpCircle className="h-4 w-4" />;
    case 'paused':
      return <Pause className="h-4 w-4" />;
    case 'completed':
      return <CheckCircle className="h-4 w-4" />;
    case 'terminated':
      return <Square className="h-4 w-4" />;
    default:
      return null;
  }
}

function StateVariant(
  state: string
): 'default' | 'secondary' | 'destructive' | 'outline' {
  switch (state) {
    case 'executing':
      return 'default';
    case 'awaiting_input':
      return 'secondary';
    case 'completed':
      return 'outline';
    case 'terminated':
      return 'destructive';
    default:
      return 'outline';
  }
}

export function AgentSessionStatus({
  session,
  isConnected,
  error,
  onPause,
  onResume,
  onTerminate,
}: AgentSessionStatusProps) {
  if (!session) {
    return (
      <div className="flex items-center gap-2 p-4 text-muted-foreground">
        <Loader2 className="h-4 w-4 animate-spin" />
        Loading session...
      </div>
    );
  }

  const state = session.state;
  const isTerminal = state === 'completed' || state === 'terminated';
  const isExecuting = state === 'executing';
  const isPaused = state === 'paused';
  const canPause = isExecuting;
  const canResume = isPaused;
  const canTerminate = !isTerminal;

  return (
    <div className="flex flex-col gap-2 p-4 border-b">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          {/* Connection status */}
          <div
            className={cn(
              'flex items-center gap-1 text-xs',
              isConnected ? 'text-green-600' : 'text-red-600'
            )}
          >
            {isConnected ? (
              <Wifi className="h-3 w-3" />
            ) : (
              <WifiOff className="h-3 w-3" />
            )}
            {isConnected ? 'Connected' : 'Disconnected'}
          </div>

          {/* Session state badge */}
          <Badge variant={StateVariant(state)} className="gap-1">
            <StateIcon state={state} />
            <span className="capitalize">{state.replace('_', ' ')}</span>
          </Badge>

          {/* Agent type badge */}
          <Badge variant="outline" className="text-xs">
            {session.agent_type}
          </Badge>
        </div>

        {/* Action buttons */}
        <div className="flex items-center gap-2">
          {canPause && (
            <Button variant="outline" size="sm" onClick={onPause}>
              <Pause className="h-4 w-4 mr-1" />
              Pause
            </Button>
          )}
          {canResume && (
            <Button variant="outline" size="sm" onClick={onResume}>
              <Play className="h-4 w-4 mr-1" />
              Resume
            </Button>
          )}
          {canTerminate && (
            <Button variant="destructive" size="sm" onClick={onTerminate}>
              <Square className="h-4 w-4 mr-1" />
              Terminate
            </Button>
          )}
        </div>
      </div>

      {/* Error display */}
      {error && (
        <div className="flex items-center gap-2 text-sm text-red-600 bg-red-50 dark:bg-red-950 p-2 rounded">
          <AlertCircle className="h-4 w-4 flex-shrink-0" />
          <span>{error}</span>
        </div>
      )}

      {/* Session error from backend */}
      {session.error_message && (
        <div className="flex items-center gap-2 text-sm text-red-600 bg-red-50 dark:bg-red-950 p-2 rounded">
          <AlertCircle className="h-4 w-4 flex-shrink-0" />
          <span>{session.error_message}</span>
        </div>
      )}

      {/* Pending question indicator */}
      {session.pending_question && (
        <div className="flex items-center gap-2 text-sm text-blue-600 bg-blue-50 dark:bg-blue-950 p-2 rounded">
          <HelpCircle className="h-4 w-4 flex-shrink-0" />
          <span>Agent is waiting for your response</span>
        </div>
      )}

      {/* Session metadata */}
      <div className="flex items-center gap-4 text-xs text-muted-foreground">
        <span>Session: {session.id.slice(0, 8)}...</span>
        {session.working_dir && <span>Dir: {session.working_dir}</span>}
        <span>
          Created: {new Date(session.created_at).toLocaleString()}
        </span>
        {session.completed_at && (
          <span>
            Completed: {new Date(session.completed_at).toLocaleString()}
          </span>
        )}
      </div>
    </div>
  );
}
