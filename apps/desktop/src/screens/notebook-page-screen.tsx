import {
  Badge,
  Button,
  Card,
  CardContent,
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogTrigger,
  Input,
} from '@yanuka/ui';
import { ArrowRight, Save, Sparkles } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { Link, useNavigate, useParams } from 'react-router-dom';
import { toast } from 'sonner';
import { useRepository } from '../lib/repository';
import {
  ocrGetPage,
  ocrLexicon,
  ocrSaveNote,
  ocrSetTokenText,
  type OcrPageDetail,
  type OcrToken,
} from '../lib/desktop-io';

/**
 * The transcription workbench: the scanned page beside its words.
 *
 * Every box the segmentation found appears as an input in reading order;
 * typing a word teaches the writer memory, and boxes the memory recognizes
 * arrive pre-filled and marked. The page becomes a contact's note when the
 * user says so — never automatically, because a wrong guess in this archive
 * costs more than a missing word.
 */
export function NotebookPageScreen() {
  const { id = '' } = useParams();
  const navigate = useNavigate();
  const repository = useRepository();

  const [page, setPage] = useState<OcrPageDetail | null>(null);
  const [drafts, setDrafts] = useState<Record<string, string>>({});
  const [active, setActive] = useState<string | null>(null);
  const [suggestions, setSuggestions] = useState<string[]>([]);
  const [contactQuery, setContactQuery] = useState('');
  const [contactMatches, setContactMatches] = useState<
    { id: string; label: string }[]
  >([]);

  useEffect(() => {
    void ocrGetPage(id).then((detail) => {
      setPage(detail);
      setDrafts(
        Object.fromEntries(detail.tokens.map((token) => [token.id, token.text ?? ''])),
      );
    });
  }, [id]);

  const mergeTokens = useCallback((tokens: OcrToken[]) => {
    setPage((current) => (current ? { ...current, tokens } : current));
    setDrafts((current) => {
      const next = { ...current };
      for (const token of tokens) {
        // A learned fill lands in the draft only if the user isn't mid-edit.
        if (!next[token.id]) {
          next[token.id] = token.text ?? '';
        }
      }
      return next;
    });
  }, []);

  const commit = async (token: OcrToken) => {
    const text = drafts[token.id] ?? '';
    if ((token.text ?? '') === text.trim()) {
      return;
    }
    try {
      mergeTokens(await ocrSetTokenText(token.id, text));
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'השמירה נכשלה');
    }
  };

  const searchContacts = async (text: string) => {
    setContactQuery(text);
    if (!text.trim()) {
      setContactMatches([]);
      return;
    }
    const response = await repository.search({
      text,
      sort: 'relevance',
      limit: 5,
      offset: 0,
      favoritesOnly: false,
      includeDeleted: false,
    });
    setContactMatches(
      response.results.map((result) => ({
        id: result.contact.id,
        label: result.contact.displayName,
      })),
    );
  };

  const saveTo = async (contactId: string) => {
    try {
      await ocrSaveNote(page?.id ?? id, contactId);
      toast.success('הדף נשמר כהערה בכרטיס');
      navigate(`/contacts/${contactId}`);
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'השמירה נכשלה');
    }
  };

  if (!page) {
    return null;
  }

  const lines = new Map<number, OcrToken[]>();
  for (const token of page.tokens) {
    const line = lines.get(token.lineIndex) ?? [];
    line.push(token);
    lines.set(token.lineIndex, line);
  }

  return (
    <div className="mx-auto max-w-6xl space-y-4 p-6">
      <div className="flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-xl font-semibold">
          <Link to="/notebooks" className="text-muted-foreground hover:text-foreground">
            <ArrowRight className="size-5" aria-hidden />
          </Link>
          {page.fileName}
        </h1>
        <Dialog>
          <DialogTrigger asChild>
            <Button data-testid="notebook-save-note">
              <Save className="size-4" aria-hidden />
              שמירה כהערה בכרטיס
            </Button>
          </DialogTrigger>
          <DialogContent>
            <DialogHeader>
              <DialogTitle>לאיזה איש קשר לצרף את הדף?</DialogTitle>
            </DialogHeader>
            <Input
              autoFocus
              placeholder="חיפוש איש קשר…"
              value={contactQuery}
              onChange={(event) => void searchContacts(event.target.value)}
            />
            <div className="space-y-1">
              {contactMatches.map((match) => (
                <Button
                  key={match.id}
                  variant="ghost"
                  className="w-full justify-start"
                  onClick={() => void saveTo(match.id)}
                >
                  {match.label}
                </Button>
              ))}
            </div>
          </DialogContent>
        </Dialog>
      </div>

      <div className="grid gap-4 lg:grid-cols-2">
        <Card>
          <CardContent className="p-3">
            <div className="relative">
              <img src={page.imageDataUrl} alt={page.fileName} className="w-full rounded" />
              {page.tokens.map((token) => (
                <button
                  key={token.id}
                  type="button"
                  aria-label={`מילה בשורה ${token.lineIndex + 1}`}
                  onClick={() => {
                    setActive(token.id);
                    document.getElementById(`token-${token.id}`)?.focus();
                  }}
                  className={`absolute rounded-sm border-2 ${
                    active === token.id
                      ? 'border-primary'
                      : token.source === 'manual'
                        ? 'border-emerald-500/70'
                        : token.source === 'learned'
                          ? 'border-sky-500/70'
                          : 'border-amber-400/60'
                  }`}
                  style={{
                    left: `${(token.x / page.width) * 100}%`,
                    top: `${(token.y / page.height) * 100}%`,
                    width: `${(token.w / page.width) * 100}%`,
                    height: `${(token.h / page.height) * 100}%`,
                  }}
                />
              ))}
            </div>
          </CardContent>
        </Card>

        <Card>
          <CardContent className="space-y-4 p-4">
            {[...lines.entries()].map(([lineIndex, tokens]) => (
              <div key={lineIndex} className="space-y-1">
                <p className="text-xs text-muted-foreground">שורה {lineIndex + 1}</p>
                <div className="flex flex-wrap gap-2">
                  {tokens.map((token) => (
                    <div key={token.id} className="space-y-1">
                      <Input
                        id={`token-${token.id}`}
                        list="ocr-lexicon"
                        dir="rtl"
                        className={`w-32 ${
                          token.source === 'learned' ? 'border-sky-500/70' : ''
                        }`}
                        value={drafts[token.id] ?? ''}
                        data-testid={`token-input-${token.id}`}
                        onFocus={() => setActive(token.id)}
                        onChange={(event) => {
                          const value = event.target.value;
                          setDrafts((current) => ({ ...current, [token.id]: value }));
                          void ocrLexicon(value).then(setSuggestions);
                        }}
                        onBlur={() => void commit(token)}
                        onKeyDown={(event) => {
                          if (event.key === 'Enter') {
                            event.currentTarget.blur();
                          }
                        }}
                      />
                      {token.source === 'learned' ? (
                        <Badge variant="secondary" className="gap-1 text-xs">
                          <Sparkles className="size-3" aria-hidden />
                          זוהה מהכתב
                        </Badge>
                      ) : null}
                    </div>
                  ))}
                </div>
              </div>
            ))}
            <datalist id="ocr-lexicon">
              {suggestions.map((term) => (
                <option key={term} value={term} />
              ))}
            </datalist>
            <p className="text-xs text-muted-foreground">
              מילה שמתוקנת כאן מלמדת את המערכת: אותה צורת כתב תזוהה מעצמה בהמשך,
              ותסומן "זוהה מהכתב".
            </p>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
