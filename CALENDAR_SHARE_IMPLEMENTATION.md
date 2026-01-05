# Calendar Sharing via iCal Link - Implementation Guide

## Overview

This implementation adds the ability to share calendars with users who don't have accounts by generating shareable iCal links with embedded secrets. The implementation follows the existing patterns used for app passwords in Stalwart.

## Architecture

### Data Model

Calendar share links are stored as `PrincipalData::CalendarShareLink(String)` entries in the user's principal. The format is:

```
$cal$<calendar_id>|<description>|<created_at>|<last_used>$<hashed_secret>
```

Example:
```
$cal$work|Work Calendar Link|1704067200||$argon2id$v=19$m=19456,t=2,p=1$...
```

**Components:**
- `calendar_id`: The ID of the calendar being shared (e.g., "work", "personal")
- `description`: User-provided description for the link (e.g., "Convention schedule for visitors")
- `created_at`: Unix timestamp when the link was created
- `last_used`: Unix timestamp when the link was last accessed (optional, can be empty)
- `hashed_secret`: Argon2 hash of the secret token for verification

### API Endpoints

#### Management API (Authenticated)

**GET /api/account/calendar-shares**
- Returns list of all calendar share links for the authenticated user
- Response format:
```json
{
  "data": [
    {
      "id": "abc123...",
      "calendar_id": "work",
      "description": "Work Calendar Link",
      "createdAt": 1704067200,
      "lastUsed": 1704070800,
      "secret": null,  // Never returned for security
      "url": null
    }
  ]
}
```

**POST /api/account/calendar-shares**
- Creates or deletes calendar share links
- Request formats:

Create:
```json
{
  "type": "create",
  "calendar_id": "work",
  "description": "Convention schedule for visitor QR code"
}
```

Response includes the plain secret (only time it's visible):
```json
{
  "data": {
    "id": "abc123...",
    "calendar_id": "work",
    "description": "Convention schedule for visitor QR code",
    "secret": "ABCD1234EFGH5678...",  // Base32-encoded, only shown once
    "createdAt": 1704067200,
    "lastUsed": null,
    "url": "https://mail.example.com/calendar/share/ABCD1234EFGH5678..."
  }
}
```

Delete:
```json
{
  "type": "delete",
  "share_id": "abc123..."
}
```

#### Public API (Unauthenticated)

**GET /calendar/share/{secret}**
- Returns iCal feed for the shared calendar
- No authentication required - secret in URL provides authorization
- Response: `text/calendar` with iCal content

Example URL: `https://mail.example.com/calendar/share/ABCD1234EFGH5678IJKL9012`

## Implementation Details

### Files Modified

1. **crates/directory/src/lib.rs**
   - Added `CalendarShareLink(String)` variant to `PrincipalData` enum

2. **crates/directory/src/backend/internal/mod.rs**
   - Added `is_calendar_share_link()` method to `SpecialSecrets` trait
   - Returns true for secrets starting with `$cal$`

3. **crates/directory/src/backend/internal/manage.rs**
   - Updated principal creation to handle `CalendarShareLink`
   - Updated principal update (Set/AddItem/RemoveItem) operations
   - Added handling in `map_principal()` for serialization

4. **crates/directory/src/backend/sql/lookup.rs**
   - Updated SQL principal lookup to handle `CalendarShareLink` secrets

5. **crates/directory/src/backend/ldap/lookup.rs**
   - Updated LDAP principal lookup to handle `CalendarShareLink` secrets

6. **crates/http/src/management/principal.rs**
   - Added `CalendarShareRequest` enum for API requests
   - Added `CalendarShareLink` struct for API responses
   - Implemented `handle_calendar_shares_get()` method
   - Implemented `handle_calendar_shares_post()` method
   - Added methods to `PrincipalManager` trait

7. **crates/http/src/management/mod.rs**
   - Added route handlers for `/api/account/calendar-shares` GET and POST

8. **crates/http/src/request.rs**
   - Added route for `/calendar/share/{secret}`
   - Added import for `CalendarShareHandler` trait

9. **crates/http/src/calendar_share.rs** (NEW)
   - Created module for public iCal endpoint
   - Implemented `CalendarShareHandler` trait
   - Contains placeholder for full implementation

10. **crates/http/src/lib.rs**
    - Added `pub mod calendar_share;`

### Security Considerations

1. **Secret Generation:**
   - Uses `utils::config::utils::generate_random_bytes::<32>()` for cryptographic randomness
   - 32 bytes = 256 bits of entropy
   - Base32-encoded for URL-safe representation

2. **Secret Storage:**
   - Secrets are hashed using Argon2 (via `utils::config::utils::hash_secret()`)
   - Only the hash is stored in the database
   - Original secret is shown once at creation time, never retrievable

3. **Access Control:**
   - Management endpoints require `Permission::ManagePasswords`
   - Public endpoint has no authentication (secret provides authorization)
   - Rate limiting should be added for public endpoint

## What Needs to Be Completed

### 1. Public iCal Endpoint Implementation

The `handle_calendar_share_ical()` method in `calendar_share.rs` needs to be completed. Current challenges:

**Challenge A: Finding the Principal by Share Link**

Currently, there's no efficient way to find which principal owns a given share link. Options:

**Option 1: Iterate all principals (current placeholder)**
- Simple but inefficient
- Works for small deployments
- Not scalable

**Option 2: Create indexed share link collection**
- Add a new collection type for share links
- Index by secret hash → (account_id, calendar_id)
- Requires more extensive changes but better performance

**Option 3: Add search capability to Directory**
- Extend `QueryParams` to support searching by secret
- Update all directory backends
- More consistent with existing architecture

**Recommended: Option 2** - Create a separate indexed collection

### 2. Calendar Event Export to iCal

Need to implement conversion of calendar events to iCal format:

```rust
async fn handle_calendar_share_ical(&self, secret: &str) -> trc::Result<HttpResponse> {
    // 1. Find the principal and calendar for this share link
    let (account_id, calendar_id) = self.find_share_link(secret).await?;
    
    // 2. Fetch calendar collection
    let resources = self
        .fetch_dav_resources_public(account_id, SyncCollection::Calendar)
        .await?;
    
    // 3. Build iCal content
    let mut ical = String::from("BEGIN:VCALENDAR\r\n");
    ical.push_str("VERSION:2.0\r\n");
    ical.push_str("PRODID:-//Stalwart//Calendar Share//EN\r\n");
    
    for resource in resources.iter() {
        if resource.collection_id().to_string() == calendar_id && !resource.is_container() {
            // Fetch event data
            let event = self.fetch_calendar_event(account_id, resource.document_id()).await?;
            
            // Convert to iCal format
            ical.push_str(&event.to_ical_format());
        }
    }
    
    ical.push_str("END:VCALENDAR\r\n");
    
    // 4. Update last_used timestamp (optional)
    self.update_share_link_last_used(account_id, secret).await?;
    
    // 5. Return response
    Ok(HttpResponse::new(StatusCode::OK)
        .with_header("Content-Type", "text/calendar; charset=utf-8")
        .with_header("Content-Disposition", "inline; filename=\"calendar.ics\"")
        .with_header("Cache-Control", "private, max-age=300")
        .with_body(ical))
}
```

### 3. Testing

Create tests for:

1. **Share link creation:**
   - Verify secret generation
   - Verify metadata storage
   - Verify hashing

2. **Share link listing:**
   - Verify secrets are never returned
   - Verify correct filtering by user

3. **Share link deletion:**
   - Verify removal
   - Verify 404 for non-existent links

4. **Public endpoint:**
   - Verify secret validation
   - Verify iCal format
   - Verify calendar filtering
   - Verify rate limiting

5. **Integration tests:**
   - Test with real calendar clients (Thunderbird, iOS, Android)
   - Test subscription updates
   - Test concurrent access

### 4. UI Integration

For self-service portal integration:

1. **List view:**
   - Show all share links with calendar name, description, created date, last used
   - Copy URL to clipboard button
   - Delete button with confirmation

2. **Create view:**
   - Calendar selector dropdown
   - Description field with examples
   - Generate button
   - Show QR code of URL (useful for physical postings)
   - One-time display of secret/URL

### 5. Additional Features (Optional)

1. **Expiration dates:**
   - Add `expires_at` field to metadata
   - Check expiration in public endpoint
   - UI to set expiration when creating

2. **Access logging:**
   - Log each access with IP, user agent, timestamp
   - Show in UI

3. **Rate limiting:**
   - Implement per-secret rate limiting
   - Prevent abuse of public endpoint

4. **Calendar filtering:**
   - Support date ranges in URL parameters
   - Support event type filtering

## Testing the Implementation

Once compilation is complete:

### 1. Manual Testing

```bash
# Create a share link
curl -X POST https://mail.example.com/api/account/calendar-shares \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"type":"create","calendar_id":"work","description":"Test link"}'

# List share links
curl https://mail.example.com/api/account/calendar-shares \
  -H "Authorization: Bearer YOUR_TOKEN"

# Access public endpoint (after implementation complete)
curl https://mail.example.com/calendar/share/SECRET_FROM_CREATION

# Delete a share link
curl -X POST https://mail.example.com/api/account/calendar-shares \
  -H "Authorization: Bearer YOUR_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"type":"delete","share_id":"abc123..."}'
```

### 2. Calendar Client Testing

1. **Thunderbird:**
   - Right-click calendars
   - New Calendar → On the Network
   - Paste iCal URL
   - Verify events appear

2. **iOS:**
   - Settings → Calendar → Accounts
   - Add Account → Other → Add Subscribed Calendar
   - Paste URL
   - Verify events sync

3. **Google Calendar:**
   - Settings → Add calendar → From URL
   - Paste URL
   - Verify events appear

## Migration Path

For existing deployments:

1. Deploy code with new PrincipalData variant
2. Existing principals without share links work unchanged
3. Users create share links as needed via API or UI
4. No database migration required

## Performance Considerations

1. **Share link lookup:**
   - Current: O(n) where n = number of principals
   - With indexing: O(1) hash lookup
   - Critical for deployments with many users

2. **iCal generation:**
   - Cache generated iCal for short periods (5 minutes)
   - Use ETags for conditional requests
   - Consider partial responses for large calendars

3. **Public endpoint:**
   - No authentication overhead
   - Add CDN caching if heavily used
   - Rate limit per secret to prevent abuse

## Future Enhancements

1. **Granular permissions:**
   - Read-only vs read-write share links
   - Event field filtering (hide attendees, locations, etc.)

2. **Multiple calendar support:**
   - Share multiple calendars via single link
   - Calendar groups

3. **Write access:**
   - Allow event creation via shared link
   - Moderation queue for changes

4. **Analytics:**
   - Track unique visitors
   - Popular events
   - Access patterns
