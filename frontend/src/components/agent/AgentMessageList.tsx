import { useRef, useEffect } from 'react';
import type { AgentMessage } from 'shared/types';
import { cn } from '@/lib/utils';
import { User, Bot, Wrench, Terminal, Info } from 'lucide-react';

interface AgentMessageListProps {
  messages: AgentMessage[];
  className?: string;
}

function MessageIcon({ role }: { role: string }) {
  switch (role) {
    case 'user':
      return <User className="h-4 w-4" />;
    case 'assistant':
      return <Bot className="h-4 w-4" />;
    case 'tool_use':
      return <Wrench className="h-4 w-4" />;
    case 'tool_result':
      return <Terminal className="h-4 w-4" />;
    case 'system':
      return <Info className="h-4 w-4" />;
    default:
      return null;
  }
}

function MessageBubble({ message }: { message: AgentMessage }) {
  const isUser = message.role === 'user';
  const isAssistant = message.role === 'assistant';
  const isTool = message.role === 'tool_use' || message.role === 'tool_result';
  const isSystem = message.role === 'system';

  return (
    <div
      className={cn(
        'flex gap-2 p-3 rounded-lg',
        isUser && 'bg-primary/10 ml-8',
        isAssistant && 'bg-muted mr-8',
        isTool && 'bg-yellow-500/10 text-sm font-mono',
        isSystem && 'bg-blue-500/10 text-sm'
      )}
    >
      <div
        className={cn(
          'flex-shrink-0 w-6 h-6 rounded-full flex items-center justify-center',
          isUser && 'bg-primary text-primary-foreground',
          isAssistant && 'bg-secondary',
          isTool && 'bg-yellow-500/20',
          isSystem && 'bg-blue-500/20'
        )}
      >
        <MessageIcon role={message.role} />
      </div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2 mb-1">
          <span className="text-xs font-medium capitalize">
            {message.role.replace('_', ' ')}
          </span>
          <span className="text-xs text-muted-foreground">
            {new Date(message.created_at).toLocaleTimeString()}
          </span>
          {message.token_count && (
            <span className="text-xs text-muted-foreground">
              ({message.token_count} tokens)
            </span>
          )}
        </div>
        <div className="whitespace-pre-wrap break-words text-sm">
          {message.content}
        </div>
        {message.metadata && (
          <details className="mt-2">
            <summary className="text-xs text-muted-foreground cursor-pointer">
              Metadata
            </summary>
            <pre className="text-xs mt-1 p-2 bg-muted rounded overflow-x-auto">
              {typeof message.metadata === 'string'
                ? message.metadata
                : JSON.stringify(message.metadata, null, 2)}
            </pre>
          </details>
        )}
      </div>
    </div>
  );
}

export function AgentMessageList({
  messages,
  className,
}: AgentMessageListProps) {
  const bottomRef = useRef<HTMLDivElement>(null);

  // Auto-scroll to bottom when new messages arrive
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, [messages.length]);

  if (messages.length === 0) {
    return (
      <div
        className={cn(
          'flex items-center justify-center text-muted-foreground p-8',
          className
        )}
      >
        No messages yet. Send a query to start the conversation.
      </div>
    );
  }

  return (
    <div className={cn('flex flex-col gap-3 p-4 overflow-y-auto', className)}>
      {messages.map((message) => (
        <MessageBubble key={message.id} message={message} />
      ))}
      <div ref={bottomRef} />
    </div>
  );
}
