import { Badge, Button, Card, CardContent, CardHeader, CardTitle } from '@yanuka/ui';
import { NotebookPen, Trash2, Upload } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { Link } from 'react-router-dom';
import { toast } from 'sonner';
import {
  ocrAvailable,
  ocrDeletePage,
  ocrImportPage,
  ocrListPages,
  type OcrPageSummary,
} from '../lib/desktop-io';

/**
 * The notebook shelf: imported scans and how far their transcription got.
 *
 * The pages live inside the encrypted database, so this list *is* the
 * archive's scanned wing — deleting here deletes the stored scan (the note it
 * produced, if any, stays with its contact).
 */
export function NotebooksScreen() {
  const [pages, setPages] = useState<OcrPageSummary[]>([]);
  const [importing, setImporting] = useState(false);
  const fileInput = useRef<HTMLInputElement>(null);

  const refresh = useCallback(() => {
    void ocrListPages().then(setPages);
  }, []);
  useEffect(refresh, [refresh]);

  const onFiles = async (files: FileList | null) => {
    if (!files || files.length === 0) {
      return;
    }
    if (!ocrAvailable()) {
      toast.error('ייבוא מחברות זמין באפליקציית המחשב');
      return;
    }
    setImporting(true);
    try {
      for (const file of Array.from(files)) {
        const buffer = new Uint8Array(await file.arrayBuffer());
        let binary = '';
        for (let i = 0; i < buffer.length; i += 0x8000) {
          binary += String.fromCharCode(...buffer.subarray(i, i + 0x8000));
        }
        await ocrImportPage(file.name, btoa(binary));
      }
      toast.success(files.length === 1 ? 'הדף נקלט' : `${files.length} דפים נקלטו`);
      refresh();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'הייבוא נכשל');
    } finally {
      setImporting(false);
      if (fileInput.current) {
        fileInput.current.value = '';
      }
    }
  };

  const remove = async (page: OcrPageSummary) => {
    try {
      await ocrDeletePage(page.id);
      toast.success('הדף נמחק');
      refresh();
    } catch (error) {
      toast.error(error instanceof Error ? error.message : 'המחיקה נכשלה');
    }
  };

  return (
    <div className="mx-auto max-w-3xl space-y-4 p-6">
      <div className="flex items-center justify-between">
        <h1 className="flex items-center gap-2 text-xl font-semibold">
          <NotebookPen className="size-5" aria-hidden />
          מחברות
        </h1>
        <div>
          <input
            ref={fileInput}
            type="file"
            accept="image/jpeg,image/png,image/webp"
            multiple
            hidden
            onChange={(event) => void onFiles(event.target.files)}
          />
          <Button
            onClick={() => fileInput.current?.click()}
            disabled={importing}
            data-testid="notebook-import"
          >
            <Upload className="size-4" aria-hidden />
            {importing ? 'קולט…' : 'ייבוא דפים סרוקים'}
          </Button>
        </div>
      </div>

      <p className="text-sm text-muted-foreground">
        המערכת מחלקת כל דף למילים, ולומדת את כתב היד מהתיקונים: מילה שתוקנה פעם
        אחת מזוהה מעצמה בדפים הבאים. הסריקות נשמרות בתוך המאגר המוצפן.
      </p>

      {pages.length === 0 ? (
        <Card>
          <CardContent className="py-10 text-center text-sm text-muted-foreground">
            אין עדיין דפים. אפשר לייבא צילום או סריקה של עמוד מחברת (JPG/PNG).
          </CardContent>
        </Card>
      ) : (
        pages.map((page) => (
          <Card key={page.id} data-testid="notebook-page-card">
            <CardHeader className="pb-2">
              <CardTitle className="flex items-center justify-between text-base">
                <Link
                  to={`/notebooks/${page.id}`}
                  className="hover:underline"
                  data-testid="notebook-page-link"
                >
                  {page.fileName}
                </Link>
                <span className="flex items-center gap-2">
                  {page.status === 'done' ? (
                    <Badge variant="secondary">נשמר כהערה</Badge>
                  ) : page.filled > 0 ? (
                    <Badge variant="outline">בתמלול</Badge>
                  ) : (
                    <Badge variant="outline">חדש</Badge>
                  )}
                  <Button
                    variant="ghost"
                    size="icon"
                    aria-label="מחיקת הדף"
                    onClick={() => void remove(page)}
                  >
                    <Trash2 className="size-4" aria-hidden />
                  </Button>
                </span>
              </CardTitle>
            </CardHeader>
            <CardContent className="text-sm text-muted-foreground">
              {page.filled} מתוך {page.tokens} מילים תומללו
            </CardContent>
          </Card>
        ))
      )}
    </div>
  );
}
