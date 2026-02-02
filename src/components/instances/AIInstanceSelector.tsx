import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { ChevronDown, Plus } from 'lucide-react';
import { useInstanceStore } from '@/stores/instanceStore';
import { CreateInstanceDialog } from './CreateInstanceDialog';
import { cn } from '@/utils/cn';

export const AIInstanceSelector = () => {
  const { t } = useTranslation();
  const { instances, activeInstance, createInstance, switchInstance } = useInstanceStore();
  const [isOpen, setIsOpen] = useState(false);
  const [isDialogOpen, setIsDialogOpen] = useState(false);
  
  const handleCreate = async (name: string) => {
    await createInstance(name);
    setIsDialogOpen(false);
  };
  
  const handleSwitch = async (id: string) => {
    await switchInstance(id);
    setIsOpen(false);
  };
  
  return (
    <>
      <div className="relative">
        {/* Trigger Button */}
        <button
          onClick={() => setIsOpen(!isOpen)}
          className={cn(
            'flex items-center gap-2 px-3 py-1.5 rounded-lg',
            'text-sm font-sans text-muted',
            'hover:bg-surface hover:text-foreground',
            'transition-colors'
          )}
        >
          <span>{activeInstance?.name || t('ai_instances.switch')}</span>
          <ChevronDown className="w-4 h-4" />
        </button>
        
        {/* Dropdown */}
        {isOpen && (
          <>
            {/* Backdrop */}
            <div
              className="fixed inset-0 z-10"
              onClick={() => setIsOpen(false)}
            />
            
            {/* Menu */}
            <div className="absolute top-full mt-2 left-0 z-20 bg-surface border border-border rounded-lg shadow-lg min-w-50 py-2 animate-slide-down">
              {/* Instance List */}
              {instances.map((instance) => (
                <button
                  key={instance.id}
                  onClick={() => handleSwitch(instance.id)}
                  className={cn(
                    'w-full px-4 py-2 text-left text-sm font-sans',
                    'hover:bg-background transition-colors',
                    'flex items-center justify-between',
                    activeInstance?.id === instance.id && 'bg-background'
                  )}
                >
                  <span>{instance.name}</span>
                  {activeInstance?.id === instance.id && (
                    <span className="text-xs text-success">{t('ai_instances.active')}</span>
                  )}
                </button>
              ))}
              
              {/* Separator */}
              {instances.length > 0 && (
                <div className="h-px bg-border my-2" />
              )}
              
              {/* Create New */}
              <button
                onClick={() => {
                  setIsOpen(false);
                  setIsDialogOpen(true);
                }}
                className="w-full px-4 py-2 text-left text-sm font-sans hover:bg-background transition-colors flex items-center gap-2 text-accent"
              >
                <Plus className="w-4 h-4" />
                <span>{t('ai_instances.create_new')}</span>
              </button>
            </div>
          </>
        )}
      </div>
      
      {/* Create Dialog */}
      <CreateInstanceDialog
        isOpen={isDialogOpen}
        onClose={() => setIsDialogOpen(false)}
        onCreate={handleCreate}
      />
    </>
  );
};
