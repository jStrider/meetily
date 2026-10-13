'use client';

import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { FolderOpen } from 'lucide-react';
import { Switch } from './ui/switch';

interface SummaryAutoSavePreferences {
  enabled: boolean;
  folder: string;
}

export function SummaryAutoSaveSettings() {
  const [preferences, setPreferences] = useState<SummaryAutoSavePreferences | null>(null);

  useEffect(() => {
    invoke<SummaryAutoSavePreferences>('get_summary_auto_save_preferences')
      .then(setPreferences)
      .catch((error) => console.error('Failed to load summary auto-save preferences:', error));
  }, []);

  const save = async (next: SummaryAutoSavePreferences) => {
    try {
      await invoke('set_summary_auto_save_preferences', { preferences: next });
      setPreferences(next);
    } catch (error) {
      console.error('Failed to save summary auto-save preferences:', error);
      toast.error('Failed to save auto-save settings');
    }
  };

  const handleChooseFolder = async () => {
    if (!preferences) return;
    const folder = await invoke<string | null>('select_summary_auto_save_folder');
    if (folder) {
      await save({ ...preferences, folder });
    }
  };

  const handleOpenFolder = async () => {
    try {
      await invoke('open_summary_auto_save_folder');
    } catch (error) {
      console.error('Failed to open summary folder:', error);
      toast.error('Failed to open folder');
    }
  };

  if (!preferences) return null;

  return (
    <div className="bg-white rounded-lg border border-gray-200 p-6 shadow-sm">
      <div className="flex items-center justify-between">
        <div>
          <h3 className="text-lg font-semibold text-gray-900 mb-2">Auto-save Summary</h3>
          <p className="text-sm text-gray-600">
            Save each summary as a Markdown file when it is generated or edited
          </p>
        </div>
        <Switch
          checked={preferences.enabled}
          onCheckedChange={(enabled) => save({ ...preferences, enabled })}
        />
      </div>

      {preferences.enabled && (
        <div className="mt-4 p-4 border rounded-lg bg-gray-50">
          <div className="font-medium mb-2">Save Location</div>
          <div className="text-sm text-gray-600 mb-3 break-all">{preferences.folder}</div>
          <div className="flex gap-2">
            <button
              onClick={handleChooseFolder}
              className="px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors"
            >
              Choose…
            </button>
            <button
              onClick={handleOpenFolder}
              className="flex items-center gap-2 px-3 py-2 text-sm border border-gray-300 rounded-md hover:bg-gray-100 transition-colors"
            >
              <FolderOpen className="w-4 h-4" />
              Open Folder
            </button>
          </div>
          <div className="text-xs text-gray-500 mt-3">
            Files are named YYYY-MM-DD - Meeting title.md and overwritten when the summary changes.
          </div>
        </div>
      )}
    </div>
  );
}
