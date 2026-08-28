; Hebrew translation of Tauri's custom NSIS strings.
;
; Tauri 2.x embeds translations for 22 languages — Hebrew is not one of them.
; Requesting `languages: ["Hebrew"]` still activates NSIS's core Hebrew (the
; standard buttons and pages), but every string Tauri adds on top — the whole
; "already installed, choose an operation" page among them — resolves to an
; empty string, which is exactly the blank options dialog users saw.
;
; The keys must match Tauri's embedded English.nsh verbatim, including the
; misspelled `choowHowToInstall`.
LangString addOrReinstall ${LANG_HEBREW} "הוספה או התקנה מחדש של רכיבים"
LangString alreadyInstalled ${LANG_HEBREW} "התוכנה כבר מותקנת"
LangString alreadyInstalledLong ${LANG_HEBREW} "${PRODUCTNAME} ${VERSION} כבר מותקן. יש לבחור את הפעולה הרצויה וללחוץ על 'הבא' כדי להמשיך."
LangString appRunning ${LANG_HEBREW} "{{product_name}} פועל כעת! יש לסגור אותו ולנסות שוב."
LangString appRunningOkKill ${LANG_HEBREW} "{{product_name}} פועל כעת!$\nלחיצה על 'אישור' תסגור אותו"
LangString chooseMaintenanceOption ${LANG_HEBREW} "יש לבחור את פעולת התחזוקה לביצוע."
LangString choowHowToInstall ${LANG_HEBREW} "יש לבחור כיצד להתקין את ${PRODUCTNAME}."
LangString createDesktop ${LANG_HEBREW} "יצירת קיצור דרך בשולחן העבודה"
LangString dontUninstall ${LANG_HEBREW} "לא להסיר את ההתקנה הקיימת"
LangString dontUninstallDowngrade ${LANG_HEBREW} "לא להסיר (חזרה לגרסה ישנה ללא הסרה מבוטלת במתקין הזה)"
LangString failedToKillApp ${LANG_HEBREW} "סגירת {{product_name}} נכשלה. יש לסגור אותו ידנית ולנסות שוב"
LangString installingWebview2 ${LANG_HEBREW} "מתקין את WebView2..."
LangString newerVersionInstalled ${LANG_HEBREW} "גרסה חדשה יותר של ${PRODUCTNAME} כבר מותקנת! לא מומלץ להתקין גרסה ישנה יותר. אם בכל זאת רוצים בכך, מוטב להסיר תחילה את הגרסה הקיימת. יש לבחור את הפעולה הרצויה וללחוץ על 'הבא' כדי להמשיך."
LangString older ${LANG_HEBREW} "ישנה יותר"
LangString olderOrUnknownVersionInstalled ${LANG_HEBREW} "גרסה $R4 של ${PRODUCTNAME} מותקנת במחשב. מומלץ להסיר את הגרסה הקיימת לפני ההתקנה. יש לבחור את הפעולה הרצויה וללחוץ על 'הבא' כדי להמשיך."
LangString silentDowngrades ${LANG_HEBREW} "חזרה לגרסה ישנה מבוטלת במתקין הזה, ולכן לא ניתן להמשיך בהתקנה שקטה. יש להשתמש במתקין הגרפי.$\n"
LangString unableToUninstall ${LANG_HEBREW} "ההסרה נכשלה!"
LangString uninstallApp ${LANG_HEBREW} "הסרת ${PRODUCTNAME}"
LangString uninstallBeforeInstalling ${LANG_HEBREW} "הסרת ההתקנה הקיימת ואז התקנה"
LangString unknown ${LANG_HEBREW} "לא ידועה"
LangString webview2AbortError ${LANG_HEBREW} "התקנת WebView2 נכשלה! התוכנה אינה יכולה לפעול בלעדיו. יש להפעיל את המתקין מחדש."
LangString webview2DownloadError ${LANG_HEBREW} "שגיאה: הורדת WebView2 נכשלה - $0"
LangString webview2DownloadSuccess ${LANG_HEBREW} "רכיב ההתקנה של WebView2 הורד בהצלחה"
LangString webview2Downloading ${LANG_HEBREW} "מוריד את רכיב ההתקנה של WebView2..."
LangString webview2InstallError ${LANG_HEBREW} "שגיאה: התקנת WebView2 נכשלה עם קוד יציאה $1"
LangString webview2InstallSuccess ${LANG_HEBREW} "WebView2 הותקן בהצלחה"
LangString deleteAppData ${LANG_HEBREW} "מחיקת נתוני התוכנה"
