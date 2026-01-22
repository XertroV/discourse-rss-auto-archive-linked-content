# Cookie Setup for YouTube Downloads

This guide explains how to export cookies from YouTube to bypass bot detection when downloading videos with yt-dlp.

## Why Cookies Are Needed

YouTube may show a "Sign in to confirm you're not a bot" error when downloading videos programmatically. Using cookies from an authenticated browser session allows yt-dlp to appear as a logged-in user, bypassing this restriction.

## Automatic (recommended for Docker): Persisted Browser Profile

If you're running this project via Docker Compose, you can avoid exporting `cookies.txt` entirely by:

1. Starting the included `cookie-browser` (noVNC) once
2. Logging into the sites you want (YouTube, Reddit, etc)
3. Letting the archiver use yt-dlp's `--cookies-from-browser ...` against the persisted profile directory

This repo's Docker setup stores a Chromium profile under `/cookies/chromium-profile` inside the `cookie-browser` container, and that same data is visible to the archiver at `/app/cookies/chromium-profile`.

Enable it by setting:

```bash
YT_DLP_COOKIES_FROM_BROWSER=chromium+basictext:/app/cookies/chromium-profile
```

Then:

```bash
./dc-cookies-browser.sh
# log in via http://127.0.0.1:7900 (pw: secret)
./dc-restart.sh
```

Notes:

- The `+basictext` keyring mode is often the simplest for containers.
- You can change the browser/keyring/profile spec; see `yt-dlp --help` for `--cookies-from-browser`.

## Quick Export Method

### Step 1: Open YouTube in Your Browser

1. Open your browser and navigate to [youtube.com](https://www.youtube.com)
2. Make sure you're logged in to your YouTube account
3. Open the browser's Developer Console:
   - **Chrome/Edge**: Press `F12` or `Ctrl+Shift+J` (Windows/Linux) / `Cmd+Option+J` (Mac)
   - **Firefox**: Press `F12` or `Ctrl+Shift+K` (Windows/Linux) / `Cmd+Option+K` (Mac)
   - **Safari**: Enable Developer menu first, then press `Cmd+Option+C`

### Step 2: Run the Export Script

Copy and paste the following JavaScript code into the console and press Enter:

```javascript
// Export YouTube cookies to JSON format compatible with yt-dlp
(function() {
  // Get all cookies for the current domain
  const cookies = document.cookie.split(';').map(c => {
    const [name, ...valueParts] = c.trim().split('=');
    const value = valueParts.join('=');
    if (!name || !value) return null;

    // Get cookie attributes from browser (if available)
    const cookieString = `${name}=${value}`;

    return {
      name: name.trim(),
      value: decodeURIComponent(value),
      domain: window.location.hostname.includes('youtube.com') ? '.youtube.com' : window.location.hostname,
      path: '/',
      expires: Math.floor(Date.now() / 1000) + (365 * 24 * 60 * 60), // 1 year from now
      secure: window.location.protocol === 'https:',
      httpOnly: false,
      sameSite: 'None'
    };
  }).filter(c => c !== null);

  // Export as array format (yt-dlp compatible)
  const cookieArray = cookies;

  // Create download link
  const blob = new Blob([JSON.stringify(cookieArray, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = 'yt-cookies.json';
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);

  console.log(`✓ Exported ${cookies.length} cookies to yt-cookies.json`);
  console.log('Cookie names:', cookies.map(c => c.name).join(', '));
  console.log('\nNote: HttpOnly cookies cannot be accessed via JavaScript.');
  console.log('If you encounter issues, try using a browser extension or yt-dlp --cookies-from-browser');
})();
```

### Step 3: Save the File

1. The script will automatically download `yt-cookies.json` to your browser's default download folder
2. Move this file to the same directory as your `docker-compose.yml` file
3. The file will be automatically mounted into the Docker container

## Alternative: Using Browser Extensions

If the JavaScript method doesn't capture all cookies (some may be HttpOnly), you can use browser extensions:

### Chrome/Edge
- **EditThisCookie** or **Cookie-Editor**: Export cookies in JSON format
- **Get cookies.txt LOCALLY**: Exports in Netscape format (also works with yt-dlp)

### Firefox
- **Cookie-Editor**: Export cookies in JSON format
- **cookies.txt**: Exports in Netscape format

## Using yt-dlp to Export Cookies

You can also use yt-dlp itself to export cookies from your browser:

```bash
# Export from Firefox
yt-dlp --cookies-from-browser firefox --cookies yt-cookies.json https://www.youtube.com/watch?v=dQw4w9WgXcQ

# Export from Chrome
yt-dlp --cookies-from-browser chrome --cookies yt-cookies.json https://www.youtube.com/watch?v=dQw4w9WgXcQ

# Export from Brave
yt-dlp --cookies-from-browser brave --cookies yt-cookies.json https://www.youtube.com/watch?v=dQw4w9WgXcQ
```

**Note:** This method requires yt-dlp to be installed locally and may not work if your browser is currently running (especially Chrome/Chromium).

## Cookie File Format

yt-dlp supports two cookie formats:

1. **JSON format** (recommended): The format exported by the JavaScript snippet above
2. **Netscape format** (`.txt`): Traditional cookies.txt format

Both formats work with yt-dlp's `--cookies` flag.

## TikTok Sensitive Content

TikTok marks some videos as "sensitive" with the message **"This post may not be comfortable for some audiences"**. These videos require you to be logged in to view them. Without cookies, they will fail to archive.

### Why TikTok Login Is Needed

TikTok uses login status to:
- **Verify age**: Sensitive content is restricted to logged-in users who have confirmed their age
- **Apply content preferences**: User accounts can set content sensitivity preferences
- **Prevent scraping**: TikTok blocks unauthenticated access to certain content

When the archiver encounters a TikTok sensitive content error, it will mark the archive with status `auth_required` instead of permanently skipping it. Once you configure cookies, these archives can be retried.

### Setting Up TikTok Cookies

**Method 1: Browser Profile (Recommended)**

The easiest approach for Docker setups:

1. Start the cookie browser:
   ```bash
   ./dc-cookies-browser.sh
   ```

2. Access noVNC at [http://127.0.0.1:7900](http://127.0.0.1:7900) (password: `secret`)

3. In the browser, navigate to [tiktok.com](https://www.tiktok.com) and **log in**

4. After logging in, verify you can view sensitive content by:
   - Finding a video marked "This post may not be comfortable for some audiences"
   - Confirming you can view it without being asked to log in

5. Set the environment variable in your `.env` or `docker-compose.yml`:
   ```bash
   YT_DLP_COOKIES_FROM_BROWSER=chromium+basictext:/app/cookies/chromium-profile
   ```

6. Restart the archiver:
   ```bash
   ./dc-restart.sh
   ```

**Method 2: Export Cookies.txt**

If you prefer the manual cookie file approach:

1. Install the **"Get cookies.txt LOCALLY"** browser extension for [Chrome](https://chrome.google.com/webstore/detail/get-cookiestxt-locally/cclelndahbckbenkjhflpdbgdldlbecc) or [Firefox](https://addons.mozilla.org/en-US/firefox/addon/cookies-txt/)

2. Log in to [tiktok.com](https://www.tiktok.com) in your browser

3. Verify you can view sensitive content

4. Click the extension icon and export cookies for `tiktok.com`

5. Save the exported file as `cookies.txt` in your project directory

6. Set the environment variable:
   ```bash
   COOKIES_FILE_PATH=/app/cookies.txt
   ```

7. Ensure the volume mount is configured in `docker-compose.yml`:
   ```yaml
   volumes:
     - ./cookies.txt:/app/cookies.txt
   ```

### Required TikTok Cookies

The key cookies needed for TikTok authentication include:

- `sessionid` or `sessionid_ss`: Main session identifier
- `sid_guard`, `sid_tt`: Additional session guards
- `uid_tt`: User ID
- `store-idc`, `store-country-code`: Location preferences

These are automatically included when you log in via browser.

### Retrying Auth-Required Archives

If you've already attempted to archive TikTok sensitive content without cookies, those archives will be marked as `auth_required`. To retry them after configuring cookies:

1. **Check for auth-required archives**:
   ```sql
   sqlite3 archiver.db "SELECT id, link_id FROM archives WHERE status = 'auth_required' LIMIT 10"
   ```

2. **Reset them to pending** (they'll be automatically picked up by the archiver):
   ```sql
   sqlite3 archiver.db "UPDATE archives SET status = 'pending', retry_count = 0 WHERE status = 'auth_required'"
   ```

3. Watch the logs to confirm they're being retried:
   ```bash
   docker-compose logs -f archiver | grep TikTok
   ```

### Verifying Cookie Configuration

To check if cookies are configured correctly, look for this log message when TikTok archives start:

```
Starting TikTok archive url=https://www.tiktok.com/... cookies_configured=true
```

If `cookies_configured=false`, the archiver will attempt to download without cookies (which will fail for sensitive content).

### TikTok Cookie Expiration

TikTok sessions can expire, typically after:
- **30 days** of inactivity
- When you log out from any device
- When TikTok forces a password reset

**Signs your cookies have expired:**
- Archives start failing with "login required" errors again
- The `cookies_configured=true` log appears, but downloads still fail with auth errors

**Solution:** Re-export cookies or re-login via the cookie browser.

## Updating Cookies

Cookies expire over time. When you start seeing bot detection errors again:

1. Re-export your cookies using one of the methods above
2. Replace the `yt-cookies.json` file in your project directory
3. Restart your Docker Compose services: `docker-compose restart archiver`

## Troubleshooting

### Cookies Not Working

- **Make sure you're logged in**: Export cookies while logged into YouTube
- **Check cookie expiration**: Some cookies expire quickly; re-export if needed
- **Verify file location**: Ensure `yt-cookies.json` is in the same directory as `docker-compose.yml`
- **Check file permissions**: The file should be readable by the Docker container

### Docker Compose Volume Mount Issues

If cookies aren't working in Docker Compose:

1. **Verify file exists before starting**: Docker Compose will create a directory if the file doesn't exist:
   ```bash
   ls -la yt-cookies.json  # Should show a file, not a directory
   ```

2. **Check inside container**: Verify the file is mounted correctly:
   ```bash
   docker-compose exec archiver ls -la /app/yt-cookies.json
   docker-compose exec archiver file /app/yt-cookies.json  # Should say "JSON" not "directory"
   ```

3. **If directory was created**: Remove it and recreate the file:
   ```bash
   rm -rf yt-cookies.json  # Remove directory if it exists
   # Re-export cookies and save as yt-cookies.json
   docker-compose restart archiver
   ```

### Docker Compose Fails to Start

If Docker Compose fails with a volume mount error:
- Make sure `yt-cookies.json` exists before starting the container
- Or comment out the volume mount line in `docker-compose.yml` if you don't need cookies yet

### HttpOnly Cookies Not Captured

The JavaScript snippet cannot access HttpOnly cookies. If you need those:
- Use a browser extension that can access HttpOnly cookies
- Or use yt-dlp's `--cookies-from-browser` method
- Or use a browser automation tool like Selenium

## Security Notes

- **Keep cookies private**: Cookie files contain authentication tokens. Don't commit them to version control
- **Add to .gitignore**: Make sure `yt-cookies.json` is in your `.gitignore` file
- **Rotate regularly**: Cookies can be used to access your account; rotate them periodically
- **Use dedicated account**: Consider using a separate YouTube account for archiving to limit exposure
