# Scroll Studio hosting plan

Research checked November 1, 2026. The desktop application stays free. The current website only distributes Scroll Studio; reader accounts, uploads, coins and creator payouts belong to later phases.

## Current deployment

The owner chose their existing Contabo server and [scroll.project456.net](https://scroll.project456.net). The static download site is now served by a dedicated Nginx virtual host with Let's Encrypt HTTPS and automatic renewal. No new hosting plan was purchased. Public release downloads are independent of the private source repository. See the [deployment guide](../website/deploy/README.md).

The managed-service options below remain scaling recommendations for a future publishing platform; none of its accounts, uploads or commerce features have been deployed.

## Recommended path

**Cloudflare Workers Static Assets** is a managed alternative for global delivery. Keep HTML, CSS, small JavaScript, branding and versioned Windows downloads together. Requests served directly as static assets are free and unlimited, with no additional asset-storage charge. Avoid routing every static request through application code. [Static-asset billing](https://developers.cloudflare.com/workers/static-assets/billing-and-limitations/)

The present installer and portable ZIP are approximately 5 MB each, below the **25 MiB per-file** static-asset limit. When binaries outgrow that limit, move them to R2 while preserving the landing-page URLs. The source repository can remain private: copy only approved release binaries, checksums and public site assets into the deployment. Private GitHub release links are not anonymous public downloads. [Workers limits](https://developers.cloudflare.com/workers/platform/limits/)

The download page uses the owner's `scroll.project456.net` subdomain. Keep future media on a separate hostname. GitHub Actions can later verify a release and deploy allowlisted artifacts through a scoped deployment credential. Never expose GitHub access tokens in browser code.

## Future publishing platform

| Responsibility | Proposed service | Purpose |
| --- | --- | --- |
| Landing page, reader shell, CDN | Workers Static Assets | Keep the simple site fast while growing the platform separately. |
| Sessions, metadata, purchases, entitlements | Supabase Postgres + Auth | Managed identity, relational data and transactions. |
| Originals and prepared reader images | Cloudflare R2 | Store images outside the database and app release. |
| Authenticated API and media access | Cloudflare Workers | Validate requests, authorize uploads and check chapter ownership. |
| Resize, validate, slice, thumbnail, scan | Queue + isolated media worker/container | Process large originals outside reader requests. |
| Checkout and creator payouts | Stripe Checkout + Connect, subject to eligibility | Provider-hosted payment collection and creator onboarding. |

These are architectural recommendations, not claims that choosing providers supplies the publishing product.

Authorize short-lived direct R2 uploads, validate completed objects, enqueue processing, and publish an immutable chapter manifest only after its images are ready. Originals remain private. Generate a few display sizes once and lazy-load nearby strips. Presigned URLs grant temporary access without revealing storage credentials. [R2 presigned URLs](https://developers.cloudflare.com/r2/api/s3/presigned-urls/)

Serve public chapters through an R2 custom domain with caching. The rate-limited `r2.dev` endpoint is intended for development. Paid images should remain private behind server-checked entitlements; never expose the same objects through an unrestricted bucket URL. Cache design must not let one purchaser's authorization unlock a chapter for everyone. [R2 public buckets](https://developers.cloudflare.com/r2/buckets/public-buckets/)

Workers have a **128 MB per-isolate memory limit**, including WebAssembly. Keep large image decoding, archive inspection and possible future AI generation in bounded background jobs. Queues offer retries, dead-letter routing and external HTTP pull consumers, permitting a conventional container service later. [Workers limits](https://developers.cloudflare.com/workers/platform/limits/), [Cloudflare Queues](https://developers.cloudflare.com/queues/)

## Database and payment boundaries

A Supabase project has one primary region. Choose it near the first audience and API workload. Replicas are asynchronous and read-only: use the primary for purchases, balance changes, refunds and unlock checks needing fresh state. Additional replicas cost extra and have deployment prerequisites. [Regions](https://supabase.com/docs/guides/platform/regions), [read replicas](https://supabase.com/docs/guides/platform/read-replicas), [prerequisites](https://supabase.com/docs/guides/platform/read-replicas/getting-started)

I recommend Postgres rather than designing the future ledger around a single D1 database. D1 suits small edge metadata, but its current paid per-database limit is **10 GB**. Postgres avoids making that partition boundary part of the initial commerce design. This is a design tradeoff, not a claim that D1 cannot implement transactions. [D1 limits](https://developers.cloudflare.com/d1/platform/limits/)

Coins should be integer, platform-only chapter credits, with paid and promotional grants distinguishable. Use an append-only ledger and unique purchase/redemption identifiers. Atomically debit the wallet and create the chapter entitlement in one database transaction; record refunds and reversals as new entries. Client balances, browser storage, payment redirects and eventually consistent caches are not proof of payment.

A verified payment event grants credits once. Persist event and transaction IDs, use idempotency keys, and tolerate duplicate or out-of-order webhooks. A transactional outbox can queue notifications and accounting jobs after the database commit. [Stripe idempotency](https://docs.stripe.com/api/idempotent_requests), [webhook behavior](https://docs.stripe.com/webhooks)

Creator earnings need a separate currency-denominated payable ledger, not a cash-out reader wallet. Reconcile purchases, redemptions, refunds, shares and payouts. Connect configuration determines fee and negative-balance responsibilities. Confirm countries, funds flow and business model with Stripe before implementing paid launch: Stripe lists content platforms and stored-value products among restricted categories requiring review. Do not assume coins are automatically accepted. [Connect responsibilities](https://docs.stripe.com/connect/accounts-v2/connected-account-configuration), [restricted businesses](https://stripe.com/legal/restricted-businesses)

The upload product also needs creator ownership declarations, AI-assisted-content labels, reports, a moderation queue and chapter withdrawal while preserving purchase/accounting history. These belong in the later upload scope, not as nonfunctional controls on today's download page.

## Verified cost drivers

Published USD rates are planning inputs, before tax, domains and extra usage. Recheck when activating services.

| Component | Published baseline |
| --- | --- |
| Static site | Direct static requests are free and unlimited; no extra asset-storage charge. |
| Workers API paid plan | Minimum $5/month; 10 million requests and 30 million CPU milliseconds included. Additional usage: $0.30/million requests and $0.02/million CPU milliseconds. |
| R2 Standard | $0.015/GB-month; Class A writes $4.50/million, Class B reads $0.36/million; direct R2 egress has no fee. Monthly free tier: 10 GB-month, 1 million Class A and 10 million Class B operations. |
| Supabase Pro | Starts at $25/month; one project with 8 GB database disk, 250 GB egress and seven days of daily backups. Compute, replicas, disk, egress and optional recovery features can add costs. |

Sources: [Workers pricing](https://developers.cloudflare.com/workers/platform/pricing/), [R2 pricing](https://developers.cloudflare.com/r2/pricing/), [Supabase pricing](https://supabase.com/pricing), [static billing](https://developers.cloudflare.com/workers/static-assets/billing-and-limitations/).

The download-only site can fit the static free tier. A small publishing pilot has a roughly **$30/month base** for Workers Paid plus one Supabase Pro project, before storage/operations, media processing, email, observability, payment fees and taxes. This is not a total-service quote. R2's lack of egress fees matters for image-heavy reading, but requests, storage, transforms, jobs and API execution still cost money. Establish quotas and budget alerts before opening uploads.

## Alternative

**AWS S3 + CloudFront, with RDS Postgres, SQS and a container backend later** is credible if the team wants a conventional container/Rust server and broader AWS stack. CloudFront currently offers a $0 flat-rate tier with 1 million requests and 100 GB transfer, or a $15/month Pro tier with 10 million requests and 50 TB transfer. These are CDN allowances, not database/compute costs. Sustained excess usage can cause delivery adjustments; account eligibility applies. [CloudFront pricing](https://aws.amazon.com/cloudfront/pricing/), [plan behavior](https://docs.aws.amazon.com/AmazonCloudFront/latest/DeveloperGuide/flat-rate-pricing-plan.html)

Cloudflare remains a useful future distribution option for static downloads and comic imagery. AWS becomes attractive when media-processing/container requirements dominate. The current Contabo deployment is sufficient for the download site; expanding it into commerce would also require capacity planning, backups and recovery.

## Delivery stages

1. **Now:** free download site, real installer/portable links, version, checksum and 456C credit; no login or payment.
2. **Publishing pilot:** creator sign-in, drafts, direct uploads, processing, reader, moderation and basic analytics.
3. **Paid pilot:** provider-approved coin model, durable ledger, chapter unlocks, reconciliation and creator payouts.
4. **Measured growth:** cache tuning, database compute/read replicas and job capacity based on actual traffic.

Public download hosting is now settled on Contabo. The remaining decisions concern the future publishing platform, payment-provider eligibility and measured scaling requirements. No paid publishing service was activated.
