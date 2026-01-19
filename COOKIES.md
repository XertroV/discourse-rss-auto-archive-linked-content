# Cookie Setup for YouTube Downloads

This guide explains how to export cookies from YouTube to bypass bot detection when downloading videos with yt-dlp.

## Why Cookies Are Needed

YouTube may show a "Sign in to confirm you're not a bot" error when downloading videos programmatically. Using cookies from an authenticated browser session allows yt-dlp to appear as a logged-in user, bypassing this restriction.

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
