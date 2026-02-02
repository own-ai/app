import { useTranslation } from 'react-i18next';
import { Menu, Settings, Search } from 'lucide-react';
import { IconButton } from '@/components/ui/IconButton';
import { AIInstanceSelector } from '@/components/instances/AIInstanceSelector';

export const Header = () => {
  const { t } = useTranslation();
  
  return (
    <header className="flex items-center justify-between px-8 py-4 border-b border-border bg-background sticky top-0 z-10">
      {/* Logo */}
      <h1 className="text-xl font-serif tracking-tight">ownAI</h1>
      
      {/* AI Instance Selector */}
      <div className="flex-1 flex justify-center">
        <AIInstanceSelector />
      </div>
      
      {/* Actions */}
      <div className="flex items-center gap-3">
        <IconButton
          icon={Search}
          label={t('common.search')}
          onClick={() => console.log('Search clicked')}
        />
        <IconButton
          icon={Menu}
          label={t('common.menu')}
          onClick={() => console.log('Menu clicked')}
        />
        <IconButton
          icon={Settings}
          label={t('common.settings')}
          onClick={() => console.log('Settings clicked')}
        />
      </div>
    </header>
  );
};
