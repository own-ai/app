import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { X } from 'lucide-react';
import { Button } from '@/components/ui/Button';
import { Input } from '@/components/ui/Input';
import { IconButton } from '@/components/ui/IconButton';

interface CreateInstanceDialogProps {
  isOpen: boolean;
  onClose: () => void;
  onCreate: (name: string) => Promise<void>;
}

export const CreateInstanceDialog = ({
  isOpen,
  onClose,
  onCreate,
}: CreateInstanceDialogProps) => {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [isCreating, setIsCreating] = useState(false);
  const [error, setError] = useState('');
  
  const handleCreate = async () => {
    if (!name.trim()) {
      setError(t('ai_instances.name_placeholder'));
      return;
    }
    
    setIsCreating(true);
    setError('');
    
    try {
      await onCreate(name.trim());
      setName('');
      onClose();
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to create instance');
    } finally {
      setIsCreating(false);
    }
  };
  
  if (!isOpen) return null;
  
  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 bg-foreground/20 z-40 animate-slide-down"
        onClick={onClose}
      />
      
      {/* Dialog */}
      <div className="fixed inset-0 flex items-center justify-center z-50 p-4">
        <div className="bg-surface border border-border rounded-lg shadow-lg max-w-md w-full p-6 animate-slide-down">
          {/* Header */}
          <div className="flex items-center justify-between mb-4">
            <h2 className="text-xl font-serif">
              {t('ai_instances.create_new')}
            </h2>
            <IconButton
              icon={X}
              label={t('common.close')}
              onClick={onClose}
            />
          </div>
          
          {/* Content */}
          <div className="space-y-4">
            <Input
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder={t('ai_instances.name_placeholder')}
              error={error}
              disabled={isCreating}
              autoFocus
              onKeyDown={(e) => {
                if (e.key === 'Enter') {
                  handleCreate();
                }
              }}
            />
          </div>
          
          {/* Actions */}
          <div className="flex items-center justify-end gap-3 mt-6">
            <Button
              variant="ghost"
              onClick={onClose}
              disabled={isCreating}
            >
              {t('common.cancel')}
            </Button>
            <Button
              onClick={handleCreate}
              isLoading={isCreating}
              disabled={!name.trim() || isCreating}
            >
              {t('ai_instances.create')}
            </Button>
          </div>
        </div>
      </div>
    </>
  );
};
