import { Message as MessageType } from '@/types';
import { cn } from '@/utils/cn';
import { MessageContent } from './MessageContent';
import { useTranslation } from 'react-i18next';

interface MessageProps {
  message: MessageType;
  isStreaming?: boolean;
}

export const Message = ({ message, isStreaming = false }: MessageProps) => {
  const { t, i18n } = useTranslation();
  const isUser = message.role === 'user';
  const isSystem = message.role === 'system';
  
  // Format timestamp using current language
  const formatTime = (date: Date) => {
    return date.toLocaleTimeString(i18n.language, {
      hour: '2-digit',
      minute: '2-digit',
    });
  };
  
  return (
    <div
      className={cn(
        'py-6 px-8 group',
        isUser && 'message-user-indent', // Custom utility from app.css
        isSystem && 'bg-system-bg'
      )}
    >
      {/* Content with role-based typography */}
      <div
        className={cn(
          'max-w-none',
          isUser && 'font-sans font-medium text-user-text',
          !isUser && !isSystem && 'font-serif text-foreground leading-relaxed',
          isSystem && 'font-mono text-sm text-system'
        )}
      >
        <MessageContent content={message.content} role={message.role} />
        {/* Streaming cursor */}
        {isStreaming && (
          <span className="inline-block w-2 h-4 ml-0.5 bg-foreground animate-pulse align-text-bottom" />
        )}
      </div>
      
      {/* Metadata - Tool Calls */}
      {message.metadata?.toolCalls && message.metadata.toolCalls.length > 0 && (
        <div className="mt-3 flex items-center gap-2 text-xs font-mono text-muted">
          <span className="w-2 h-2 bg-warning rounded-full animate-pulse-scale" />
          <span>{t('chat.tools_used')}: {message.metadata.toolCalls.join(', ')}</span>
        </div>
      )}
      
      {/* Metadata - Memories */}
      {message.metadata?.memories && message.metadata.memories.length > 0 && (
        <div className="mt-3 italic text-sm text-muted">
          <span className="text-muted/60">[</span>
          {message.metadata.memories.join(', ')}
          <span className="text-muted/60">]</span>
        </div>
      )}
      
      {/* Timestamp - appears on hover */}
      <div className="mt-2 text-xs text-muted font-sans opacity-0 group-hover:opacity-100 transition-opacity">
        {formatTime(message.timestamp)}
      </div>
    </div>
  );
};
