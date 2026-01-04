import { useState, useCallback } from 'react';
import type { AgentPendingInput } from 'shared/types';
import { Button } from '@/components/ui/button';
import {
  Card,
  CardContent,
  CardDescription,
  CardFooter,
  CardHeader,
  CardTitle,
} from '@/components/ui/card';
import { Textarea } from '@/components/ui/textarea';
import { Check, X, MessageCircleQuestion, Shield } from 'lucide-react';
import { cn } from '@/lib/utils';

interface AgentPendingInputCardProps {
  input: AgentPendingInput;
  onRespond: (inputId: string, response: string) => Promise<void>;
  onApprove: (inputId: string, approved: boolean) => Promise<void>;
}

export function AgentPendingInputCard({
  input,
  onRespond,
  onApprove,
}: AgentPendingInputCardProps) {
  const [response, setResponse] = useState('');
  const [isSubmitting, setIsSubmitting] = useState(false);

  const isQuestion = input.input_type === 'question' || input.input_type === 'clarification';
  const isApproval = input.input_type === 'approval';

  const handleSubmitResponse = useCallback(async () => {
    if (!response.trim()) return;
    setIsSubmitting(true);
    try {
      await onRespond(input.id, response);
      setResponse('');
    } finally {
      setIsSubmitting(false);
    }
  }, [input.id, response, onRespond]);

  const handleApprove = useCallback(async () => {
    setIsSubmitting(true);
    try {
      await onApprove(input.id, true);
    } finally {
      setIsSubmitting(false);
    }
  }, [input.id, onApprove]);

  const handleDeny = useCallback(async () => {
    setIsSubmitting(true);
    try {
      await onApprove(input.id, false);
    } finally {
      setIsSubmitting(false);
    }
  }, [input.id, onApprove]);

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent) => {
      if (e.key === 'Enter' && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        handleSubmitResponse();
      }
    },
    [handleSubmitResponse]
  );

  return (
    <Card
      className={cn(
        'border-2',
        isApproval ? 'border-yellow-500/50' : 'border-blue-500/50'
      )}
    >
      <CardHeader className="pb-2">
        <div className="flex items-center gap-2">
          {isApproval ? (
            <Shield className="h-5 w-5 text-yellow-500" />
          ) : (
            <MessageCircleQuestion className="h-5 w-5 text-blue-500" />
          )}
          <CardTitle className="text-base">
            {isApproval ? 'Tool Approval Required' : 'Question from Agent'}
          </CardTitle>
        </div>
        {input.tool_name && (
          <CardDescription className="font-mono text-xs">
            Tool: {input.tool_name}
          </CardDescription>
        )}
      </CardHeader>

      <CardContent className="pb-2">
        <p className="text-sm whitespace-pre-wrap">{input.prompt}</p>

        {input.tool_input && (
          <div className="mt-2 p-2 bg-muted rounded-md">
            <p className="text-xs text-muted-foreground mb-1">Tool Input:</p>
            <pre className="text-xs overflow-x-auto">
              {typeof input.tool_input === 'string'
                ? input.tool_input
                : JSON.stringify(JSON.parse(input.tool_input), null, 2)}
            </pre>
          </div>
        )}

        {isQuestion && (
          <div className="mt-3">
            <Textarea
              value={response}
              onChange={(e) => setResponse(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder="Type your response..."
              disabled={isSubmitting}
              className="min-h-[80px]"
            />
          </div>
        )}
      </CardContent>

      <CardFooter className="gap-2">
        {isApproval ? (
          <>
            <Button
              variant="outline"
              size="sm"
              onClick={handleDeny}
              disabled={isSubmitting}
            >
              <X className="h-4 w-4 mr-1" />
              Deny
            </Button>
            <Button size="sm" onClick={handleApprove} disabled={isSubmitting}>
              <Check className="h-4 w-4 mr-1" />
              Approve
            </Button>
          </>
        ) : (
          <Button
            size="sm"
            onClick={handleSubmitResponse}
            disabled={isSubmitting || !response.trim()}
          >
            Send Response
          </Button>
        )}
      </CardFooter>
    </Card>
  );
}
