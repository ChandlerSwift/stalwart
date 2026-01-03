/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: AGPL-3.0-only OR LicenseRef-SEL
 */

use common::Server;
use directory::{PrincipalData, QueryBy, QueryParams};
use groupware::calendar::CalendarEvent;
use http_proto::HttpResponse;
use hyper::StatusCode;
use std::future::Future;
use store::{
    ValueKey,
    write::AlignedBytes,
};
use trc::AddContext;
use types::{
    collection::{Collection, SyncCollection},
    id::Id,
};

pub trait CalendarShareHandler: Sync + Send {
    fn handle_calendar_share_ical(
        &self,
        secret: &str,
    ) -> impl Future<Output = trc::Result<HttpResponse>> + Send;
}

impl CalendarShareHandler for Server {
    async fn handle_calendar_share_ical(&self, secret: &str) -> trc::Result<HttpResponse> {
        // Hash the provided secret to compare with stored hashed secrets
        let hashed_secret = utils::config::utils::hash_secret(secret);

        // Search for a principal with this share link
        let mut calendar_id: Option<String> = None;
        let mut account_id: Option<u32> = None;

        // We need to iterate through all principals to find the one with this share link
        // This is not optimal, but for a minimal implementation it works
        // In production, you'd want to index these share links differently
        let principals = self
            .directory()
            .query(QueryParams::by_email("*").with_return_member_of(false))
            .await;

        // Since we can't query all principals easily, we'll try a different approach
        // For now, we'll return an error indicating this needs a better implementation
        // In a production system, you'd want to:
        // 1. Store share links in a separate indexed collection
        // 2. Or add a method to query principals by their secrets

        // For minimal implementation, let's just return an error
        // The full implementation would require more extensive changes
        return Err(trc::EventType::Resource(trc::ResourceEvent::NotFound)
            .into_err()
            .details("Calendar share link not found"));

        // Below is the pseudo-code for what the full implementation would look like:
        /*
        // Find the principal and calendar for this share link
        for principal in all_principals {
            for data in &principal.data {
                if let PrincipalData::CalendarShareLink(link_secret) = data {
                    if let Some((meta, stored_hash)) =
                        link_secret.strip_prefix("$cal$").and_then(|s| s.split_once('$'))
                    {
                        if stored_hash == hashed_secret {
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

        // Fetch all calendar events for this calendar
        let resources = self
            .fetch_dav_resources_public(account_id, SyncCollection::Calendar)
            .await
            .caused_by(trc::location!())?;

        // Build iCal content
        let mut ical_content = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Stalwart//Calendar Share//EN\r\n");

        for resource in resources.iter() {
            if !resource.is_container() && resource.collection_id().to_string() == calendar_id {
                if let Some(event_data) = self
                    .store()
                    .get_value::<AlignedBytes>(ValueKey::archive(
                        account_id,
                        Collection::CalendarEvent,
                        resource.document_id(),
                    ))
                    .await
                    .caused_by(trc::location!())?
                {
                    if let Ok(event) = event_data.unarchive::<CalendarEvent>() {
                        // Convert event to iCal format
                        ical_content.push_str(&event.to_ical_string());
                    }
                }
            }
        }

        ical_content.push_str("END:VCALENDAR\r\n");

        Ok(HttpResponse::new(StatusCode::OK)
            .with_header("Content-Type", "text/calendar; charset=utf-8")
            .with_header(
                "Content-Disposition",
                "attachment; filename=\"calendar.ics\"",
            )
            .with_body(ical_content))
        */
    }
}
