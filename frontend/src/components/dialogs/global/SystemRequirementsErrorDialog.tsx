import { useState, useEffect } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { AlertCircle, RefreshCw, Download, Server } from 'lucide-react';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { defineModal } from '@/lib/modals';
import { configApi } from '@/lib/api';
import { Loader } from '@/components/ui/loader';

export interface SystemRequirementsErrorDialogProps {
  claudeCodeInstalled: boolean;
  agentServiceAvailable: boolean;
  errorMessage?: string;
}

const SystemRequirementsErrorDialogImpl =
  NiceModal.create<SystemRequirementsErrorDialogProps>(
    ({ claudeCodeInstalled, agentServiceAvailable, errorMessage }) => {
      const modal = useModal();
      const [isRetrying, setIsRetrying] = useState(false);

      // Auto-retry every 5 seconds
      useEffect(() => {
        if (!modal.visible) return;

        const interval = setInterval(async () => {
          try {
            const result = await configApi.checkSystemReadiness();
            if (result.ready) {
              modal.resolve('ready');
            }
          } catch {
            // Ignore errors during auto-retry
          }
        }, 5000);

        return () => clearInterval(interval);
      }, [modal.visible, modal]);

      const handleRetry = async () => {
        setIsRetrying(true);
        try {
          const result = await configApi.checkSystemReadiness();
          if (result.ready) {
            modal.resolve('ready');
          }
        } catch {
          // Stay on this dialog
        } finally {
          setIsRetrying(false);
        }
      };

      const showClaudeCodeError = !claudeCodeInstalled;
      const showAgentServiceError = claudeCodeInstalled && !agentServiceAvailable;

      return (
        <Dialog open={modal.visible} uncloseable={true}>
          <DialogContent className="sm:max-w-[550px]">
            <DialogHeader>
              <div className="flex items-center gap-3">
                <AlertCircle className="h-6 w-6 text-destructive" />
                <DialogTitle>System Requirements Not Met</DialogTitle>
              </div>
              <DialogDescription className="text-left space-y-4 pt-4">
                {showClaudeCodeError && (
                  <div className="p-4 bg-destructive/10 rounded-lg border border-destructive/20">
                    <div className="flex items-start gap-3">
                      <Download className="h-5 w-5 text-destructive mt-0.5" />
                      <div>
                        <p className="font-semibold text-foreground">
                          Claude Code Not Installed
                        </p>
                        <p className="text-sm mt-1">
                          Claude Code is required to run AI coding agents. Please
                          install it to continue.
                        </p>
                        <a
                          href="https://docs.anthropic.com/en/docs/claude-code/getting-started"
                          target="_blank"
                          rel="noopener noreferrer"
                          className="inline-flex items-center gap-2 mt-3 text-sm text-primary hover:underline"
                        >
                          <Download className="h-4 w-4" />
                          Install Claude Code
                        </a>
                      </div>
                    </div>
                  </div>
                )}

                {showAgentServiceError && (
                  <div className="p-4 bg-destructive/10 rounded-lg border border-destructive/20">
                    <div className="flex items-start gap-3">
                      <Server className="h-5 w-5 text-destructive mt-0.5" />
                      <div>
                        <p className="font-semibold text-foreground">
                          Agent Service Not Running
                        </p>
                        <p className="text-sm mt-1">
                          The agent service is required but not responding. Please
                          start it by running:
                        </p>
                        <code className="block mt-2 p-2 bg-muted rounded text-xs font-mono">
                          cd agent-service && npm install && npm run dev
                        </code>
                      </div>
                    </div>
                  </div>
                )}

                {errorMessage && (
                  <p className="text-sm text-muted-foreground">{errorMessage}</p>
                )}

                <p className="text-sm text-muted-foreground">
                  This dialog will automatically check every 5 seconds, or you can
                  manually retry below.
                </p>
              </DialogDescription>
            </DialogHeader>
            <DialogFooter>
              <Button
                onClick={handleRetry}
                variant="default"
                disabled={isRetrying}
              >
                {isRetrying ? (
                  <Loader size={16} className="mr-2" />
                ) : (
                  <RefreshCw className="h-4 w-4 mr-2" />
                )}
                {isRetrying ? 'Checking...' : 'Retry'}
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      );
    }
  );

export const SystemRequirementsErrorDialog = defineModal<
  SystemRequirementsErrorDialogProps,
  'ready' | void
>(SystemRequirementsErrorDialogImpl);
