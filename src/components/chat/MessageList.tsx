import { useEffect, useRef } from 'react';
import { useVirtualizer } from '@tanstack/react-virtual';
import { useTranslation } from 'react-i18next';
import { Message as MessageComponent } from './Message';
import { Message as MessageType } from '@/types';

interface MessageListProps {
  messages: MessageType[];
  isStreaming?: boolean;
}

export const MessageList = ({ messages, isStreaming = false }: MessageListProps) => {
  const { t } = useTranslation();
  const parentRef = useRef<HTMLDivElement>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  
  // Virtualizer for performance with many messages
  const virtualizer = useVirtualizer({
    count: messages.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 150, // Estimated height per message
    overscan: 5, // Number of items to render outside visible area
    measureElement: (element) => element.getBoundingClientRect().height, // Measure actual height
  });
  
  // Auto-scroll to bottom when new messages arrive or during streaming
  useEffect(() => {
    if (parentRef.current) {
      parentRef.current.scrollTop = parentRef.current.scrollHeight;
    }
  }, [messages.length]);

  // Auto-scroll during streaming (content updates)
  useEffect(() => {
    if (isStreaming && parentRef.current) {
      parentRef.current.scrollTop = parentRef.current.scrollHeight;
    }
  }, [isStreaming, messages[messages.length - 1]?.content]);
  
  if (messages.length === 0) {
    return (
      <div className="flex-1 flex items-center justify-center text-muted font-sans">
        <p>{t('chat.no_messages')}</p>
      </div>
    );
  }
  
  return (
    <div
      ref={parentRef}
      className="flex-1 overflow-y-auto"
      style={{ contain: 'strict' }}
    >
      <div
        ref={scrollRef}
        style={{
          height: `${virtualizer.getTotalSize()}px`,
          width: '100%',
          position: 'relative',
        }}
      >
        {virtualizer.getVirtualItems().map((virtualItem) => {
          const isLastMessage = virtualItem.index === messages.length - 1;
          const message = messages[virtualItem.index];
          const showStreamingCursor = isStreaming && isLastMessage && message.role === 'agent';
          
          return (
            <div
              key={virtualItem.key}
              data-index={virtualItem.index}
              ref={virtualizer.measureElement}
              style={{
                position: 'absolute',
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualItem.start}px)`,
              }}
            >
              <MessageComponent 
                message={message} 
                isStreaming={showStreamingCursor}
              />
            </div>
          );
        })}
      </div>
    </div>
  );
};
