import { useMemo, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { ArrowRight, CheckCircle2, FileUp, TriangleAlert, Upload } from 'lucide-react';
import {
  buildImportPlan,
  IMPORT_TARGET_LABELS,
  IMPORT_TARGETS,
  suggestMapping,
  type ImportTarget,
} from '@yanuka/core';
import { decodeCsvBytes, detectSeparator, parseCsv, type ParsedCsv } from '@yanuka/utils';
import {
  Alert,
  AlertDescription,
  Badge,
  Button,
  Card,
  CardContent,
  CardHeader,
  CardTitle,
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from '@yanuka/ui';
import { useImportContacts } from '../hooks/use-contacts';

const PREVIEW_ROWS = 3;

interface LoadedFile {
  name: string;
  parsed: ParsedCsv;
}

interface ImportOutcome {
  created: number;
  failures: Array<{ row: number; name: string; error: string }>;
}

/**
 * CSV import: pick a file, correct the column mapping, import.
 *
 * The mapping step is never skipped — auto-detection fills it in, but the user
 * confirms with the actual data visible under each column. Rows that cannot be
 * imported (no name, in practice) are reported with their row numbers rather
 * than aborting the rest: the archive being imported here is decades old, and
 * three malformed rows must not hold back three hundred good ones.
 */
export function ImportScreen() {
  const fileInput = useRef<HTMLInputElement>(null);
  const [file, setFile] = useState<LoadedFile | null>(null);
  const [mapping, setMapping] = useState<ImportTarget[]>([]);
  const [readError, setReadError] = useState<string | null>(null);
  const [progress, setProgress] = useState<number | null>(null);
  const [outcome, setOutcome] = useState<ImportOutcome | null>(null);
  const importContacts = useImportContacts();

  const plan = useMemo(
    () =>
      file
        ? buildImportPlan(file.parsed.rows, mapping, `ייבוא CSV — ${file.name}`)
        : [],
    [file, mapping],
  );
  const importable = plan.filter((row) => row.input !== null);

  const onFile = async (picked: File) => {
    setReadError(null);
    setOutcome(null);
    setProgress(null);
    try {
      const text = decodeCsvBytes(new Uint8Array(await picked.arrayBuffer()));
      const parsed = parseCsv(text, detectSeparator(text));
      if (parsed.headers.length === 0 || parsed.rows.length === 0) {
        setReadError('הקובץ ריק, או שאין בו שורת כותרות ולפחות שורת נתונים אחת.');
        setFile(null);
        return;
      }
      setFile({ name: picked.name, parsed });
      setMapping(suggestMapping(parsed.headers));
    } catch {
      setReadError('קריאת הקובץ נכשלה. ודא שזהו קובץ CSV.');
      setFile(null);
    }
  };

  const runImport = async () => {
    if (!file) {
      return;
    }
    setProgress(0);
    const failures: ImportOutcome['failures'] = plan
      .filter((row) => row.error !== null)
      .map((row) => ({
        row: row.row,
        name: row.input?.displayName ?? '—',
        error: row.error ?? '',
      }));
    let created = 0;
    for (const row of importable) {
      try {
        if (row.input) {
          await importContacts.create(row.input);
          created += 1;
        }
      } catch (error) {
        failures.push({
          row: row.row,
          name: row.input?.displayName ?? '—',
          error: error instanceof Error ? error.message : String(error),
        });
      }
      setProgress(created + failures.length);
    }
    importContacts.invalidate();
    failures.sort((a, b) => a.row - b.row);
    setOutcome({ created, failures });
    setProgress(null);
  };

  return (
    <div className="mx-auto max-w-4xl space-y-6 p-6">
      <div className="flex items-center gap-3">
        <Button asChild variant="ghost" size="icon" aria-label="חזרה להגדרות">
          <Link to="/settings">
            <ArrowRight className="size-4" aria-hidden />
          </Link>
        </Button>
        <h1 className="text-xl font-semibold">ייבוא אנשי קשר מקובץ CSV</h1>
      </div>

      {outcome ? (
        <ImportSummary outcome={outcome} onRestart={() => {
          setFile(null);
          setOutcome(null);
        }} />
      ) : (
        <>
          <Card>
            <CardHeader>
              <CardTitle className="flex items-center gap-2 text-base">
                <FileUp className="size-4" aria-hidden />
                קובץ
              </CardTitle>
            </CardHeader>
            <CardContent className="space-y-3">
              <p className="text-sm text-muted-foreground">
                נתמכים קבצים שיוצאו מ־Google Contacts, מ־Outlook או מגיליון Excel שנשמר
                כ־CSV. השורה הראשונה חייבת להיות שורת כותרות.
              </p>
              <input
                ref={fileInput}
                type="file"
                accept=".csv,text/csv,.txt"
                className="hidden"
                data-testid="import-file-input"
                onChange={(event) => {
                  const picked = event.target.files?.[0];
                  if (picked) {
                    void onFile(picked);
                  }
                  event.target.value = '';
                }}
              />
              <div className="flex items-center gap-3">
                <Button onClick={() => fileInput.current?.click()} disabled={progress !== null}>
                  <Upload className="size-4" aria-hidden />
                  בחירת קובץ
                </Button>
                {file ? (
                  <span className="text-sm">
                    {file.name}
                    <span className="text-muted-foreground">
                      {' '}
                      · {file.parsed.rows.length} שורות נתונים
                    </span>
                  </span>
                ) : null}
              </div>
              {readError ? (
                <Alert variant="destructive">
                  <TriangleAlert className="size-4" aria-hidden />
                  <AlertDescription>{readError}</AlertDescription>
                </Alert>
              ) : null}
            </CardContent>
          </Card>

          {file ? (
            <Card>
              <CardHeader>
                <CardTitle className="text-base">התאמת עמודות</CardTitle>
              </CardHeader>
              <CardContent className="space-y-4">
                <p className="text-sm text-muted-foreground">
                  לכל עמודה בקובץ נבחר שדה יעד. עמודות שסומנו „התעלם" לא ייובאו.
                </p>
                <div className="overflow-x-auto">
                  <Table>
                    <TableHeader>
                      <TableRow>
                        <TableHead>עמודה בקובץ</TableHead>
                        <TableHead>ייובא כ־</TableHead>
                        <TableHead>דוגמאות מהקובץ</TableHead>
                      </TableRow>
                    </TableHeader>
                    <TableBody>
                      {file.parsed.headers.map((header, index) => (
                        <TableRow key={`${header}-${index}`}>
                          <TableCell className="font-medium">{header || '(ללא כותרת)'}</TableCell>
                          <TableCell>
                            <Select
                              value={mapping[index]}
                              onValueChange={(value) =>
                                setMapping((current) => {
                                  const next = [...current];
                                  next[index] = value as ImportTarget;
                                  return next;
                                })
                              }
                            >
                              <SelectTrigger className="w-44" data-testid={`mapping-${index}`}>
                                <SelectValue />
                              </SelectTrigger>
                              <SelectContent>
                                {IMPORT_TARGETS.map((target) => (
                                  <SelectItem key={target} value={target}>
                                    {IMPORT_TARGET_LABELS[target]}
                                  </SelectItem>
                                ))}
                              </SelectContent>
                            </Select>
                          </TableCell>
                          <TableCell className="max-w-64 truncate text-sm text-muted-foreground">
                            {file.parsed.rows
                              .slice(0, PREVIEW_ROWS)
                              .map((row) => row[index])
                              .filter(Boolean)
                              .join(' · ') || '—'}
                          </TableCell>
                        </TableRow>
                      ))}
                    </TableBody>
                  </Table>
                </div>

                <div className="flex items-center gap-3">
                  <Button
                    onClick={() => void runImport()}
                    disabled={importable.length === 0 || progress !== null}
                    data-testid="run-import"
                  >
                    {progress !== null
                      ? `מייבא… ${progress} / ${plan.length}`
                      : `ייבוא ${importable.length} אנשי קשר`}
                  </Button>
                  {plan.length - importable.length > 0 ? (
                    <Badge variant="secondary">
                      {plan.length - importable.length} שורות ללא שם ידווחו בסיכום
                    </Badge>
                  ) : null}
                </div>
              </CardContent>
            </Card>
          ) : null}
        </>
      )}
    </div>
  );
}

function ImportSummary({
  outcome,
  onRestart,
}: {
  outcome: ImportOutcome;
  onRestart: () => void;
}) {
  return (
    <Card data-testid="import-summary">
      <CardHeader>
        <CardTitle className="flex items-center gap-2 text-base">
          <CheckCircle2 className="size-4 text-primary" aria-hidden />
          הייבוא הסתיים
        </CardTitle>
      </CardHeader>
      <CardContent className="space-y-4">
        <p className="text-sm">
          נוצרו <strong className="numeric">{outcome.created}</strong> אנשי קשר.
          {outcome.failures.length > 0 ? (
            <>
              {' '}
              <strong className="numeric">{outcome.failures.length}</strong> שורות לא יובאו:
            </>
          ) : null}
        </p>
        {outcome.failures.length > 0 ? (
          <div className="overflow-x-auto">
            <Table>
              <TableHeader>
                <TableRow>
                  <TableHead className="w-20">שורה</TableHead>
                  <TableHead>שם</TableHead>
                  <TableHead>סיבה</TableHead>
                </TableRow>
              </TableHeader>
              <TableBody>
                {outcome.failures.map((failure) => (
                  <TableRow key={failure.row}>
                    <TableCell className="numeric">{failure.row}</TableCell>
                    <TableCell>{failure.name}</TableCell>
                    <TableCell className="text-sm text-muted-foreground">{failure.error}</TableCell>
                  </TableRow>
                ))}
              </TableBody>
            </Table>
          </div>
        ) : null}
        <div className="flex gap-3">
          <Button asChild>
            <Link to="/contacts">לרשימת אנשי הקשר</Link>
          </Button>
          <Button variant="outline" onClick={onRestart}>
            ייבוא קובץ נוסף
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}
