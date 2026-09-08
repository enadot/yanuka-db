# איפה להריץ את השרת — השוואה והנחיה מדויקת

המסמך הזה עונה על שאלה אחת: **"מה בדיוק לרכוש ולהגדיר"** כדי שהמחשב המרכזי
והטלפון יסתנכרנו דרך שרת שתמיד זמין באינטרנט. הקובץ `yanuka-server`
(מצורף לכל שחרור) הוא השרת; כאן בוחרים לו בית.

## שלוש אפשרויות, בקצרה

| | Hostinger VPS | Railway | Hetzner Cloud |
|---|---|---|---|
| מה זה | מחשב וירטואלי (Linux) שלך | פלטפורמה שמריצה קונטיינר בשבילך | מחשב וירטואלי (Linux) שלך |
| מחיר משוער | 5–8 $ לחודש (KVM 1) | Hobby ‏5 $ לחודש + שימוש (בפועל ~5–8 $) | ~4 € לחודש (CX22) |
| מה צריך לדעת | להדביק 10 פקודות ב-SSH (מפורט למטה) | כמעט כלום — מחברים את ה-GitHub, לוחצים Deploy | כמו Hostinger |
| HTTPS | Caddy מסדר לבד (שורה אחת) | אוטומטי, כלול | Caddy |
| גיבוי | קובץ אחד לגבות; `yanuka-server backup` | Volume של Railway; גיבוי ידני דרך `backup` | כמו Hostinger |
| מיקום שרתים | יש בישראל/אירופה | אירופה/ארה"ב | גרמניה/פינלנד |

**ההמלצה:** אם אתה כבר לקוח של Hostinger ונוח לך שם — **Hostinger VPS KVM 1**.
זה הזול־שאמין, הכול בשליטתך, וההוראות למטה הן העתק־הדבק. אם אתה מעדיף לא
לגעת ב-Linux בכלל — **Railway** (מחיר דומה, פחות שליטה, פחות עבודה). Hetzner
זול יותר במעט ומצוין, אבל לא מוסיף כלום על Hostinger בשבילך.

לכל אחת מהאפשרויות התעבורה זניחה (מאגר של אלפי רשומות ושני מכשירים), כך
שהתוכנית הקטנה ביותר מספיקה בהרבה.

---

## אפשרות א׳ — Hostinger VPS (מומלץ)

### מה לרכוש

1. hostinger.com → **VPS** → תוכנית **KVM 1** (1 vCPU, 4GB RAM — יותר מדי, וזה
   בסדר). תקופה: מה שנוח; שנה זולה יותר.
2. מערכת הפעלה: **Ubuntu 24.04** (נקי, בלי פאנלים).
3. מיקום: הקרוב אליך.
4. בסיום — Hostinger מציגים **כתובת IP** וסיסמת root (או מפתח SSH). זה כל מה
   שצריך.

### דומיין (מומלץ, לא חובה)

בשביל HTTPS צריך שם. אם יש לך דומיין ב-Hostinger, מוסיפים רשומת **A**:
`otzar` → ה-IP של ה-VPS. מקבלים `otzar.your-domain.co.il`. בלי דומיין אפשר
לעבוד ב-IP + HTTP (`http://IP:8787`) — עובד, אבל לא מוצפן בדרך; לא מומלץ מעבר
לניסיון.

### הגדרה — 10 פקודות

מתחברים (מהמחשב: PowerShell או Terminal):

```sh
ssh root@<IP>
```

ואז, בשרת:

```sh
# 1. עדכונים
apt update && apt upgrade -y

# 2. משתמש ותיקייה לשרת
useradd -r -m -d /var/lib/otzar -s /usr/sbin/nologin otzar

# 3. הורדת השרת מהשחרור האחרון (החליפו את מספר הגרסה בגרסה שבדף ה-Releases)
VERSION=0.11.0
curl -L -o /usr/local/bin/yanuka-server \
  https://github.com/enadot/yanuka-db/releases/download/v$VERSION/yanuka-server_${VERSION}_linux-x64
chmod +x /usr/local/bin/yanuka-server

# 4. שירות שרץ תמיד ומתעורר אחרי ריסטארט
cat > /etc/systemd/system/yanuka-server.service <<'UNIT'
[Unit]
Description=Otzar Shlomo sync server
After=network-online.target

[Service]
User=otzar
Environment=YANUKA_DATA_DIR=/var/lib/otzar/data
Environment=YANUKA_BIND=127.0.0.1:8787
Environment=YANUKA_SERVER_NAME=אוצר שלמה
ExecStart=/usr/local/bin/yanuka-server serve
Restart=always
RestartSec=3

[Install]
WantedBy=multi-user.target
UNIT
systemctl daemon-reload
systemctl enable --now yanuka-server

# 5. HTTPS אוטומטי עם Caddy (מנפיק תעודה לבד; החליפו את שם הדומיין)
apt install -y caddy
cat > /etc/caddy/Caddyfile <<'CADDY'
otzar.your-domain.co.il {
    reverse_proxy 127.0.0.1:8787
}
CADDY
systemctl reload caddy

# 6. חומת אש: רק SSH ו-HTTPS פתוחים
ufw allow OpenSSH && ufw allow 80 && ufw allow 443 && ufw --force enable
```

### קוד הצימוד הראשון

```sh
sudo -u otzar YANUKA_DATA_DIR=/var/lib/otzar/data yanuka-server pair-code
```

מדפיס קוד תקף לרבע שעה. במחשב: הגדרות ← סנכרון בין מכשירים ← כתובת
`https://otzar.your-domain.co.il` + הקוד. את הטלפון מצמדים מהמחשב ("קוד
למכשיר נוסף").

### תחזוקה

- **לוגים:** `journalctl -u yanuka-server -f`
- **גיבוי:** `sudo -u otzar YANUKA_DATA_DIR=/var/lib/otzar/data yanuka-server backup /var/lib/otzar/backup-$(date +%F).db`
  (וגם בלי זה — כל מכשיר מצומד מחזיק עותק מלא).
- **עדכון גרסה:** להוריד את הקובץ החדש לאותו מקום (פקודה 3) ו־`systemctl restart yanuka-server`.
  השרת מריץ את המיגרציות לבד.
- **הצפנה במנוחה בשרת** (אופציונלי): לבנות עם `--features sqlcipher`
  ולהוסיף `Environment=YANUKA_DB_KEY=<64 hex>` לשירות.

---

## אפשרות ב׳ — Railway (בלי Linux)

1. railway.com → New Project → **Deploy from GitHub repo** → הריפו `yanuka-db`.
2. Settings → **Build**: Dockerfile path = `server/Dockerfile`.
3. **Variables**: `YANUKA_DATA_DIR=/data`, `YANUKA_BIND=0.0.0.0:8787`
   (Railway מזרימים לפורט הזה), `YANUKA_SERVER_NAME=אוצר שלמה`.
4. **Volume**: Add Volume → mount path `/data` (בלעדיו המאגר נמחק בכל deploy!).
5. Settings → **Networking** → Generate Domain → מקבלים `https://….up.railway.app`.
   (הפורט שמבקשים: 8787.)
6. קוד צימוד ראשון: Deploy logs מציגים אותו בהפעלה הראשונה; אחר כך —
   Railway → שירות → Shell (או `railway run`) → `yanuka-server pair-code`.

יתרון: אפס תחזוקת שרת, HTTPS מובנה. חיסרון: פחות שליטה, ומיקום השרת רחוק
יותר (זניח לסנכרון). Hobby plan ‏5 $ לחודש כולל את השימוש של שרת כזה.

---

## אפשרות ג׳ — Hetzner Cloud

זהה להוראות של Hostinger (Ubuntu 24.04, אותן 10 פקודות). התוכנית: **CX22**.
ההרשמה דורשת אימות זהות; השרתים בגרמניה/פינלנד.

---

## מה *לא* צריך

- לא Google Play, לא Firebase, לא מסד נתונים מנוהל, לא Postgres. השרת הוא קובץ
  אחד עם קובץ נתונים אחד.
- לא Tailscale ולא VPN — אלה חלופה *במקום* שרת באינטרנט, למי שמעדיף להשאיר
  את השרת בבית. ברגע שיש VPS, אין בהם צורך.
