import { open } from '@tauri-apps/plugin-dialog';
import { Copy } from 'lucide-react';
import { useEffect, useState } from 'react';
import { licenceStatus, loadLicence } from '@/bridge';
import { Field, IconButton, PanelHead, StatusBadge } from '@/design-system/kit';
import { Button } from '@/design-system/ui/button';
import { LICENCE_CHANGED, licenceStanding, type LicenceStatus, MODULE_TITLES, MODULES } from '@/licence';

const TONE = { current: 'ok', ends_soon: 'warning', grace: 'warning', viewer: 'refused' } as const;
const WORD = { current: 'Running', ends_soon: 'Ending', grace: 'In grace', viewer: 'Viewer only' } as const;

/**
 * The Licence panel (decision 0098): who this copy is licensed to, where the licence stands, the modules it runs, and
 * this machine's code to send to Zovlyn for one. A licence file is loaded from here.
 */
export function LicencePanel() {
  const [status, setStatus] = useState<LicenceStatus | null>(null);
  const [problem, setProblem] = useState<string | null>(null);
  const [copied, setCopied] = useState(false);

  useEffect(() => {
    licenceStatus().then(setStatus, (e) => setProblem(String(e)));
  }, []);

  const load = async () => {
    const path = await open({ title: 'Load a licence file', filters: [{ name: 'Zovlyn licence', extensions: ['zdlic'] }] });
    if (typeof path !== 'string') return;
    try {
      setStatus(await loadLicence(path));
      setProblem(null);
      window.dispatchEvent(new Event(LICENCE_CHANGED));
    } catch (e) {
      setProblem(String(e));
    }
  };

  const copy = async () => {
    if (!status) return;
    await navigator.clipboard.writeText(status.machine);
    setCopied(true);
    setTimeout(() => setCopied(false), 1500);
  };

  const licence = status?.licence;
  return (
    <div className="flex h-full flex-col text-xs">
      <PanelHead
        line="Who this copy is licensed to, the modules it runs, and this machine's code"
        more="With no licence, or once one has ended and its 14 days' grace are over, the app is the Viewer: projects open, show, measure, import and export, and designing, cutting and reserves runs are refused. A licence comes from Zovlyn as a .zdlic file made for this machine's code."
      />
      <div className="flex flex-col gap-1 overflow-auto p-3">
        <Button size="sm" onClick={load} className="self-start">
          Load licence file…
        </Button>
        {problem && <div className="text-destructive">{problem}</div>}
        {status && (
          <>
            <Field label="Standing">
              <StatusBadge tone={TONE[status.state]}>{WORD[status.state]}</StatusBadge>
              <span className="min-w-0">{licenceStanding(status)}</span>
            </Field>
            <Field label="Licensed to">{licence?.licensed_to ?? '–'}</Field>
            <Field label="Licence">{licence ? `${licence.licence}, issued ${licence.issued}` : '–'}</Field>
            {licence?.updates_until && <Field label="Updates">until {licence.updates_until}</Field>}
            <Field label="Modules">
              {MODULES.map((m) => (
                <StatusBadge key={m} tone={status.modules.includes(m) ? 'ok' : 'edited'} title={status.modules.includes(m) ? 'On' : 'Not in this licence'}>
                  {status.modules.includes(m) ? MODULE_TITLES[m] : <s>{MODULE_TITLES[m]}</s>}
                </StatusBadge>
              ))}
            </Field>
            <Field label="This machine" hint={copied ? 'Copied' : 'Send this code to Zovlyn for a licence for this machine.'}>
              <span className="font-mono">{status.machine}</span>
              <IconButton label="Copy this machine's code" onClick={copy}>
                <Copy />
              </IconButton>
            </Field>
            {status.path && (
              <Field label="Read from">
                <span className="min-w-0 truncate" title={status.path}>
                  {status.path}
                </span>
              </Field>
            )}
          </>
        )}
      </div>
    </div>
  );
}
