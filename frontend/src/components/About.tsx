import { BRAND } from '@/lib/brand.generated';
import React, { useState, useEffect } from "react";
import { getVersion } from '@tauri-apps/api/app';
import licenseText from '../../../LICENSE.md';
import noticesText from '../../../THIRD_PARTY_NOTICES.md';

/** Markdown shown as plain text: drop heading marks, bold and code markers. */
const plain = (markdown: string) =>
    markdown.replace(/^#+\s*/gm, '').replace(/\*\*/g, '').replace(/`/g, '').trim();

function TextBlock({ title, text }: { title: string; text: string }) {
    return (
        <details className="rounded-card border border-line bg-surface">
            <summary className="cursor-pointer px-4 py-3 text-sm font-medium text-ink">{title}</summary>
            <pre className="max-h-64 overflow-y-auto whitespace-pre-wrap px-4 pb-4 font-sans text-xs leading-relaxed text-ink-muted">{plain(text)}</pre>
        </details>
    );
}

export function About() {
    const [currentVersion, setCurrentVersion] = useState<string>('');

    useEffect(() => {
        getVersion().then(setCurrentVersion).catch(console.error);
    }, []);

    return (
        <div className="p-4 space-y-6 h-[80vh] overflow-y-auto">
            <div className="text-center space-y-2">
                {/* eslint-disable-next-line @next/next/no-img-element */}
                <img src="/mark.png" alt="" width={72} height={72} className="mx-auto" />
                <h2 className="font-serif text-3xl text-ink">{BRAND.name}</h2>
                {currentVersion && <p className="text-sm text-ink-muted">Version {currentVersion}</p>}
                <p className="text-base text-ink">{BRAND.tagline}</p>
            </div>

            <ul className="space-y-2 text-sm text-ink">
                <li>Recording, transcription and speaker detection run on this computer.</li>
                <li>Summaries use the built-in model or a local server unless you choose a cloud provider.</li>
                <li>No account, no usage statistics, no update checks.</li>
            </ul>

            <div className="space-y-3">
                <TextBlock title="MIT licence" text={licenseText} />
                <TextBlock title="Third-party notices" text={noticesText} />
            </div>
        </div>
    );
}
