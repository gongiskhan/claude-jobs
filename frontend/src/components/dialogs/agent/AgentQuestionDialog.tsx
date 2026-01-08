import { useState } from 'react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { Label } from '@/components/ui/label';
import NiceModal, { useModal } from '@ebay/nice-modal-react';
import { MessageCircleQuestion, CheckCircle2 } from 'lucide-react';
import { defineModal } from '@/lib/modals';
import { cn } from '@/lib/utils';

export interface QuestionOption {
  label: string;
  description?: string;
}

export interface AgentQuestionDialogProps {
  questionId: string;
  question: string;
  header?: string;
  options?: QuestionOption[];
  multiSelect?: boolean;
}

export interface AgentQuestionResult {
  answered: boolean;
  answer?: string;
  selectedOptions?: string[];
}

const AgentQuestionDialogImpl = NiceModal.create<AgentQuestionDialogProps>(
  (props) => {
    const modal = useModal();
    const { questionId: _questionId, question, header, options, multiSelect = false } = props;

    const [selectedOptions, setSelectedOptions] = useState<Set<string>>(
      new Set()
    );
    const [customAnswer, setCustomAnswer] = useState('');
    const [useCustomAnswer, setUseCustomAnswer] = useState(!options?.length);

    const handleOptionClick = (label: string) => {
      if (multiSelect) {
        const newSelection = new Set(selectedOptions);
        if (newSelection.has(label)) {
          newSelection.delete(label);
        } else {
          newSelection.add(label);
        }
        setSelectedOptions(newSelection);
      } else {
        if (selectedOptions.has(label)) {
          setSelectedOptions(new Set());
        } else {
          setSelectedOptions(new Set([label]));
        }
      }
      setUseCustomAnswer(false);
    };

    const handleSubmit = () => {
      let answer: string;

      if (useCustomAnswer || !options?.length) {
        answer = customAnswer;
      } else {
        const selected = Array.from(selectedOptions);
        answer = selected.join(', ');
      }

      modal.resolve({
        answered: true,
        answer,
        selectedOptions: Array.from(selectedOptions),
      } as AgentQuestionResult);
    };

    const handleCancel = () => {
      modal.resolve({
        answered: false,
      } as AgentQuestionResult);
    };

    const canSubmit =
      useCustomAnswer || !options?.length
        ? customAnswer.trim().length > 0
        : selectedOptions.size > 0;

    return (
      <Dialog open={modal.visible} onOpenChange={handleCancel}>
        <DialogContent className="sm:max-w-[500px]">
          <DialogHeader>
            <div className="flex items-center gap-3">
              <MessageCircleQuestion className="h-6 w-6 text-primary" />
              <div>
                {header && (
                  <span className="text-xs font-medium text-muted-foreground uppercase tracking-wide">
                    {header}
                  </span>
                )}
                <DialogTitle className="text-lg">{question}</DialogTitle>
              </div>
            </div>
            <DialogDescription className="text-left pt-2 sr-only">
              The agent is asking a question and waiting for your response.
            </DialogDescription>
          </DialogHeader>

          <div className="py-4 space-y-4">
            {options && options.length > 0 && (
              <div className="space-y-2">
                <Label className="text-sm text-muted-foreground">
                  {multiSelect ? 'Select all that apply' : 'Choose an option'}
                </Label>
                <div className="space-y-2">
                  {options.map((option) => (
                    <button
                      key={option.label}
                      type="button"
                      onClick={() => handleOptionClick(option.label)}
                      className={cn(
                        'w-full text-left p-3 rounded-lg border transition-colors',
                        'hover:bg-accent hover:border-primary/50',
                        selectedOptions.has(option.label)
                          ? 'border-primary bg-primary/5'
                          : 'border-border'
                      )}
                    >
                      <div className="flex items-start gap-3">
                        <div
                          className={cn(
                            'flex-shrink-0 w-5 h-5 rounded-full border-2 mt-0.5',
                            'flex items-center justify-center transition-colors',
                            selectedOptions.has(option.label)
                              ? 'border-primary bg-primary text-primary-foreground'
                              : 'border-muted-foreground/50'
                          )}
                        >
                          {selectedOptions.has(option.label) && (
                            <CheckCircle2 className="w-3 h-3" />
                          )}
                        </div>
                        <div>
                          <div className="font-medium">{option.label}</div>
                          {option.description && (
                            <div className="text-sm text-muted-foreground mt-0.5">
                              {option.description}
                            </div>
                          )}
                        </div>
                      </div>
                    </button>
                  ))}
                </div>

                <div className="pt-2">
                  <button
                    type="button"
                    onClick={() => {
                      setUseCustomAnswer(true);
                      setSelectedOptions(new Set());
                    }}
                    className={cn(
                      'text-sm text-muted-foreground hover:text-foreground transition-colors',
                      useCustomAnswer && 'text-primary font-medium'
                    )}
                  >
                    Or provide a custom answer
                  </button>
                </div>
              </div>
            )}

            {(useCustomAnswer || !options?.length) && (
              <div className="space-y-2">
                <Label htmlFor="custom-answer">Your answer</Label>
                <Textarea
                  id="custom-answer"
                  value={customAnswer}
                  onChange={(e) => setCustomAnswer(e.target.value)}
                  placeholder="Type your answer here..."
                  className="min-h-[100px]"
                  autoFocus
                />
              </div>
            )}
          </div>

          <DialogFooter className="gap-2">
            <Button variant="outline" onClick={handleCancel}>
              Skip
            </Button>
            <Button onClick={handleSubmit} disabled={!canSubmit}>
              Submit Answer
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    );
  }
);

export const AgentQuestionDialog = defineModal<
  AgentQuestionDialogProps,
  AgentQuestionResult
>(AgentQuestionDialogImpl);
