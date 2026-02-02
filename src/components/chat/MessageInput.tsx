import { useState, useRef, KeyboardEvent } from 'react';
import { useTranslation } from 'react-i18next';
import { Send } from 'lucide-react';
import { cn } from '@/utils/cn';

interface MessageInputProps {
  onSend: (message: string) => void;
  disabled?: boolean;
}

export const MessageInput = ({ onSend, disabled }: MessageInputProps) => {
  const { t } = useTranslation();
  const [value, setValue] = useState('');
  const textareaRef = useRef<HTMLTextAreaElement>(null);
  
  const handleSend = () => {
    if (!value.trim() || disabled) return;
    onSend(value);
    setValue('');
    resetHeight();
  };
  
  const handleKeyDown = (e: KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      handleSend();
    }
  };
  
  const handleInput = (e: React.FormEvent<HTMLTextAreaElement>) => {
    const target = e.currentTarget;
    target.style.height = 'auto';
    target.style.height = Math.min(target.scrollHeight, 200) + 'px';
  };
  
  const resetHeight = () => {
    if (textareaRef.current) {
      textareaRef.current.style.height = 'auto';
    }
  };
  
  return (
    <div className="flex items-end gap-3 px-8 py-4 border-t border-border bg-background">
      <textarea
        ref={textareaRef}
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onKeyDown={handleKeyDown}
        onInput={handleInput}
        placeholder={t('chat.input_placeholder')}
        disabled={disabled}
        className={cn(
          'flex-1 resize-none',
          'font-sans text-base',
          'px-4 py-3',
          'border border-border rounded-lg',
          'focus:outline-none focus:border-border-strong',
          'transition-colors',
          'min-h-14 max-h-50',
          'disabled:opacity-50 disabled:cursor-not-allowed'
        )}
        rows={1}
      />
      <button
        onClick={handleSend}
        disabled={!value.trim() || disabled}
        className={cn(
          'p-3 rounded-lg',
          'bg-foreground text-background',
          'hover:bg-accent',
          'disabled:bg-border-strong disabled:cursor-not-allowed',
          'transition-colors',
          'focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent focus-visible:ring-offset-2'
        )}
        aria-label={t('chat.send_message')}
      >
        <Send className="w-5 h-5" />
      </button>
    </div>
  );
};
