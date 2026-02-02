import { useTranslation } from 'react-i18next';

export const TypingIndicator = () => {
  const { t } = useTranslation();
  
  return (
    <div className="py-6 px-8">
      <div className="font-serif text-muted animate-pulse">
        {t('chat.agent_typing')}
      </div>
    </div>
  );
};
