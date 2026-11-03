import { useState, useEffect, useCallback } from 'react';
import { ModelConfig } from '@/components/ModelSettingsModal';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { toast } from 'sonner';
import { saveWithRemoteConfirmation } from '@/lib/cloud';

export function useModelConfiguration() {
  // Note: No hardcoded defaults - DB is the source of truth
  const [modelConfig, setModelConfig] = useState<ModelConfig>({
    provider: 'ollama',
    model: '', // Empty until loaded from DB
    whisperModel: 'large-v3'
  });
  const [isLoading, setIsLoading] = useState(true);
  const [, setError] = useState<string>('');

  // Fetch model configuration on mount
  useEffect(() => {
    const fetchModelConfig = async () => {
      setIsLoading(true);
      try {
        console.log('🔄 Fetching model configuration from database...');
        const data = await invokeTauri('api_get_model_config', {}) as any;
        if (data && data.provider !== null) {
          console.log('✅ Loaded model config from database:', {
            provider: data.provider,
            model: data.model,
            whisperModel: data.whisperModel,
            hasApiKey: !!data.hasApiKey,
            ollamaEndpoint: data.ollamaEndpoint || 'default'
          });

          // Fetch custom OpenAI config if provider is custom-openai
          if (data.provider === 'custom-openai') {
            try {
              const customConfig = await invokeTauri('api_get_custom_openai_config') as any;
              if (customConfig) {
                data.customOpenAIDisplayName = customConfig.displayName || null;
                data.customOpenAIEndpoint = customConfig.endpoint || null;
                data.customOpenAIModel = customConfig.model || null;
                data.customOpenAIApiKey = null;
                data.maxTokens = customConfig.maxTokens || null;
                data.temperature = customConfig.temperature || null;
                data.topP = customConfig.topP || null;
                // For custom-openai, model field should match customOpenAIModel
                data.model = customConfig.model || data.model;
                console.log('✅ Loaded custom OpenAI config:', {
                  displayName: customConfig.displayName,
                  endpoint: customConfig.endpoint,
                  model: customConfig.model,
                });
              }
            } catch (err) {
              console.error('Failed to fetch custom OpenAI config:', err);
            }
          }

          setModelConfig(data);
        } else {
          console.warn('⚠️ No model config found in database, using defaults');
        }
      } catch (error) {
        console.error('❌ Failed to fetch model config:', error);
      } finally {
        setIsLoading(false);
        console.log('✅ Model configuration loading complete');
      }
    };

    fetchModelConfig();
  }, []);

  // Listen for model config updates from other components
  useEffect(() => {
    const setupListener = async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const unlisten = await listen<ModelConfig>('model-config-updated', (event) => {
        console.log('Meeting details received model-config-updated event:', event.payload);
        setModelConfig(event.payload);
      });

      return unlisten;
    };

    let cleanup: (() => void) | undefined;
    setupListener().then(fn => cleanup = fn);

    return () => {
      cleanup?.();
    };
  }, []);

  // Save model configuration
  const handleSaveModelConfig = useCallback(async (updatedConfig?: ModelConfig) => {
    try {
      const configToSave = updatedConfig || modelConfig;
      const payload = {
        provider: configToSave.provider,
        model: configToSave.model,
        whisperModel: configToSave.whisperModel,
        apiKey: configToSave.apiKey ?? null,
        ollamaEndpoint: configToSave.ollamaEndpoint ?? null
      };

      // A new address on another computer is confirmed first (spec F-MOD-7)
      const saved = await saveWithRemoteConfirmation((confirmRemote) =>
        invokeTauri('api_save_model_config', {
          provider: payload.provider,
          model: payload.model,
          whisperModel: payload.whisperModel,
          apiKey: payload.apiKey,
          ollamaEndpoint: payload.ollamaEndpoint,
          confirmRemote,
        }),
      );
      if (saved === null) return false;

      console.log('Save model config success');
      // The typed key went to the core; nothing else keeps it (spec F-MOD-6)
      const withoutKey = { ...payload, apiKey: null };
      setModelConfig(withoutKey);

      // Emit event to sync other components
      const { emit } = await import('@tauri-apps/api/event');
      await emit('model-config-updated', withoutKey);

      toast.success("Summary settings Saved successfully");
      return true;
    } catch (error) {
      console.error('Failed to save model config:', error);
      toast.error("Failed to save summary settings", { description: String(error) });
      if (error instanceof Error) {
        setError(error.message);
      } else {
        setError('Failed to save model config: Unknown error');
      }
      return false;
    }
  }, [modelConfig]);

  return {
    modelConfig,
    setModelConfig,
    handleSaveModelConfig,
    isLoading,
  };
}
