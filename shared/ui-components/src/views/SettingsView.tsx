import { type ReactElement } from 'react';
import type { LayerSettings, SystemSettings, ThemeMode } from '@aurora/shared-types';
import clsx from 'clsx';

export interface SettingsViewProps {
  settings: SystemSettings;
  onChange?: (settings: SystemSettings) => void;
  className?: string;
}

const THEME_MODES: ThemeMode[] = ['light', 'dark', 'sepia', 'high_contrast', 'auto'];

function formatBinding(modifiers: string[], key: string): string {
  return [...modifiers, key].join('+');
}

function toggleLayerValue(
  settings: SystemSettings,
  layer: LayerSettings,
  key: string,
  value: boolean,
): SystemSettings {
  const layers = settings.layers.map((l) =>
    l.layer === layer.layer && l.workspace_id === layer.workspace_id
      ? { ...l, values: { ...l.values, [key]: value } }
      : l,
  );
  return { ...settings, layers };
}

/** Renders system settings: theme picker, shortcut list, and toggle switches. */
export function SettingsView({ settings, onChange, className }: SettingsViewProps): ReactElement {
  const userLayer = settings.layers.find((l) => l.layer === 'user');
  const toggles = userLayer
    ? Object.entries(userLayer.values).filter(([, v]) => typeof v === 'boolean')
    : [];

  const setTheme = (mode: ThemeMode): void => onChange?.({ ...settings, theme_mode: mode });

  const handleToggle = (key: string, current: boolean): void => {
    if (userLayer) onChange?.(toggleLayerValue(settings, userLayer, key, !current));
  };

  return (
    <section className={clsx('aurora-settings-view', className)} aria-label="Settings">
      <div className="aurora-settings-theme">
        <h3>Theme</h3>
        <div className="aurora-theme-picker" role="radiogroup" aria-label="Theme mode">
          {THEME_MODES.map((mode) => (
            <button
              key={mode}
              type="button"
              role="radio"
              aria-checked={settings.theme_mode === mode}
              className={clsx('aurora-theme-option', settings.theme_mode === mode && 'active')}
              onClick={() => setTheme(mode)}
            >
              {mode}
            </button>
          ))}
        </div>
      </div>

      <div className="aurora-settings-toggles">
        <h3>Toggles</h3>
        {toggles.length === 0 ? (
          <p className="aurora-settings-empty">No toggle preferences configured.</p>
        ) : (
          <ul>
            {toggles.map(([key, value]) => (
              <li key={key} className="aurora-toggle-row">
                <span>{key}</span>
                <button
                  type="button"
                  role="switch"
                  aria-checked={value === true}
                  className={clsx('aurora-toggle-switch', value === true && 'on')}
                  onClick={() => handleToggle(key, value === true)}
                >
                  <span className="aurora-toggle-knob" />
                </button>
              </li>
            ))}
          </ul>
        )}
      </div>

      <div className="aurora-settings-shortcuts">
        <h3>Shortcuts</h3>
        {settings.shortcuts.length === 0 ? (
          <p className="aurora-settings-empty">No shortcuts configured.</p>
        ) : (
          <table>
            <thead>
              <tr>
                <th>Name</th>
                <th>Binding</th>
                <th>Platform</th>
              </tr>
            </thead>
            <tbody>
              {settings.shortcuts.map((s) => (
                <tr key={s.id}>
                  <td>{s.name}</td>
                  <td>
                    <kbd>{formatBinding(s.binding.modifiers, s.binding.key)}</kbd>
                  </td>
                  <td>{s.platform}</td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </section>
  );
}
