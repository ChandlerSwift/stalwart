/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-SEL
 */

use common::{Server, auth::AccessToken};
use directory::{PrincipalData, Type, backend::internal::manage::ManageDirectory, core::secret::verify_secret_hash};
use groupware::{GroupwareCache, calendar::CalendarEvent};
use http_proto::HttpResponse;
use hyper::StatusCode;
use std::future::Future;
use store::{
    ValueKey,
    write::{AlignedBytes, Archive},
};
use trc::AddContext;
use types::collection::{Collection, SyncCollection};

pub trait CalendarShareHandler: Sync + Send {
    fn handle_calendar_share_ical(
        &self,
        secret: &str,
    ) -> impl Future<Output = trc::Result<HttpResponse>> + Send;
}

impl CalendarShareHandler for Server {
    async fn handle_calendar_share_ical(&self, secret: &str) -> trc::Result<HttpResponse> {
        // Search for a principal with this share link
        let mut calendar_id: Option<String> = None;
        let mut account_id: Option<u32> = None;

        // List all principals (this is not optimal, but works for a functional implementation)
        // In production, you'd want to index these share links in a separate collection
        let principals = self
            .store()
            .list_principals(
                None,
                None,
                &[Type::Individual],
                true,
                0,
                0,
            )
            .await
            .caused_by(trc::location!())?;

        // Find the principal and calendar for this share link
        for principal in principals.items {
            for data in &principal.data {
                if let PrincipalData::CalendarShareLink(link_secret) = data {
                    if let Some((meta, stored_hash)) =
                        link_secret.strip_prefix("$cal$").and_then(|s| s.split_once('$'))
                    {
                        // Compare the hashed secret
                        if verify_secret_hash(stored_hash, secret)
                            .await
                            .unwrap_or(false)
                        {
                            let parts: Vec<&str> = meta.split('|').collect();
                            if parts.len() >= 3 {
                                calendar_id = Some(parts[0].to_string());
                                account_id = Some(principal.id);
                                break;
                            }
                        }
                    }
                }
            }
            if calendar_id.is_some() {
                break;
            }
        }

        let calendar_id = calendar_id.ok_or_else(|| {
            trc::EventType::Resource(trc::ResourceEvent::NotFound)
                .into_err()
                .details("Calendar share link not found")
        })?;

        let account_id = account_id.ok_or_else(|| {
            trc::EventType::Resource(trc::ResourceEvent::NotFound)
                .into_err()
                .details("Calendar share link not found")
        })?;

        // Create a simple access token for internal use (as the account owner)
        let access_token = AccessToken::from_id(account_id);

        // Fetch all calendar events for this account
        let resources = self
            .fetch_dav_resources(&access_token, account_id, SyncCollection::Calendar)
            .await
            .caused_by(trc::location!())?;

        // Build iCal content
        let mut ical_content = String::from(
            "BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Stalwart//Calendar Share//EN\r\n",
        );

        // Iterate through resources to find events in the shared calendar
        for resource in resources.iter() {
            if !resource.is_container() 
                && resource.collection_id().to_string() == calendar_id
            {
                // Fetch the event data
                if let Some(event_archive) = self
                    .store()
                    .get_value::<Archive<AlignedBytes>>(ValueKey::archive(
                        account_id,
                        Collection::CalendarEvent,
                        resource.document_id(),
                    ))
                    .await
                    .caused_by(trc::location!())?
                {
                    if let Ok(event) = event_archive.unarchive::<CalendarEvent>() {
                        // Convert event to iCal format and append
                        ical_content.push_str(&event.data.event.to_string());
                    }
                }
            }
        }

        ical_content.push_str("END:VCALENDAR\r\n");

        Ok(HttpResponse::new(StatusCode::OK)
            .with_header("Content-Type", "text/calendar; charset=utf-8")
            .with_header(
                "Content-Disposition",
                "inline; filename=\"calendar.ics\"",
            )
            .with_header("Cache-Control", "private, max-age=300")
            .with_body(ical_content))
    }
}
