# Demand for Rowla

Research date: **October 4, 2026**, America/New_York.
This report uses public product pages, GitHub repository metadata, and selected public user discussions.
It is desk research. It does not include user interviews, a representative survey, or verified competitor revenue.

## Recommendation

Continue with a focused free beta. The category has credible demand, but Rowla has no independent traction evidence yet.
Position Rowla around a visible button for each window, a predictable Sort order, and reliable controls with measured resource use.
Free source and Rust are useful trust signals. They do not establish a unique market position.

A useful product sentence is: **A free taskbar for Mac, with each window in reach and your preferred order one click away.**

## Evidence and confidence

| Signal | Observation | What it supports | Limit |
| --- | --- | --- | --- |
| Closest taskbar product | Taskbar's publisher reports **13,000+ downloads** | People try persistent per-window taskbars | Publisher claim. Cumulative downloads are not active users or paid customers |
| Adjacent open-source product | DockDoor has **6,251 stars and 233 forks** | Interest in Dock previews and window controls | A different interaction model. Stars are not installations |
| Adjacent window switcher | AltTab has **16,362 stars and 886 forks** | Strong interest in window-level navigation | A keyboard switcher, rather than a persistent taskbar |
| Commercial supply | Several products offer trials and one-time licenses | Developers see a market for these utilities | Asking prices do not prove sales or profit |
| User comments | Windows migrants, developers, and multi-display users describe unmet needs | Specific workflows for a beta cohort | Selected public comments are not representative |
| Rowla baseline | **0 stars and 0 forks** at this research snapshot | The project is newly public | No independent demand or retention claim is justified |

The download claim comes from [Taskbar's own site](https://lawand.io/taskbar/).
GitHub counts come from [DockDoor](https://github.com/ejbills/DockDoor), [AltTab](https://github.com/lwouis/alt-tab-macos), and [Rowla](https://github.com/sharifhsn/rowla).
The exact API snapshot is [recorded here](research/github-interest-2026-10-04.json).
Do not add the counts together. The audiences overlap, and each count measures a different action.

**Assessment:** confidence is high that the problem category exists. Confidence in Rowla's adoption and willingness to pay remains low.
There is no defensible app-specific market-size estimate from these public signals.

## Competitive landscape

Prices and requirements are observations from the linked publisher pages on the research date. They can change.

| Product | Main use | Price or model | Requirements and distribution | Implication for Rowla |
| --- | --- | --- | --- | --- |
| [Taskbar](https://lawand.io/taskbar/) | Persistent button for each window, previews, Start | Free today. Publisher announces $25 one-time from December 19, 2026 | macOS 10.13+, Intel and Apple Silicon, Homebrew | Closest direct competitor. Rowla needs a clear workflow reason to switch |
| [uBar](https://ubarapp.com/) | Dock or taskbar, grouping, displays, app order | $30 personal or $50 commercial, one-time. 14-day trial | macOS 10.13+, mature product | App order and per-window buttons are established features, rather than unique claims |
| [Sidebar](https://sidebarapp.net/buy/) | Customizable Dock replacement | €19.99 lifetime, or €1.25 monthly / €12.50 yearly. 7-day trial | macOS 13+ | Visual customization already has capable suppliers |
| [DockDoor](https://github.com/ejbills/DockDoor) | Previews and Alt+Tab added to the native Dock | Free and open source. Separate Pro product | macOS 13+, both architectures, notarized download, Homebrew, automatic updates | Free, private, native previews alone do not differentiate Rowla |
| [DockDoor Pro](https://pro.dockdoor.net/) | Full Dock replacement | $20 one-time for three Macs, as stated by the developer | Separate commercial app | Another price reference for this category, not evidence of Rowla revenue |
| [AltTab](https://alt-tab.app/) | Keyboard navigation across windows | Free open-source edition and a separate Pro offering | Mature switcher with a broad audience | Rowla can target a persistent visual workflow rather than recreate AltTab |
| Rowla | Per-window strip, explicit configured Sort, hover close | Free MIT source and a free beta download | Apple Silicon, macOS 26+, development certificate, manual updates | A narrower installation audience and more first-launch friction |

Taskbar's future price is an announced plan. This report does not treat it as a completed change or competitor sales evidence.

## What users ask for

Selected comments in [this taskbar discussion](https://www.reddit.com/r/MacOS/comments/zsezns/is_there_a_taskbar_for_mac/) describe Windows-to-Mac friction and difficulty with many IDE or document windows.
They also request stable relative app positions and grouping choices.
These comments support an interview topic about predictable navigation. They do not establish how many Mac users want it.

A [2026 Taskbar discussion](https://www.reddit.com/r/macapps/comments/1tv5obf/taskbar_161_start_menu_added_to_fix_the_macos_26/) includes a three-display professional workflow, price sensitivity, and requests for compact appearance.
It also includes strong rejection of a Windows-like visual style.
**Inference:** target people who already want a taskbar. Do not market it as an improvement that every Mac user needs.

A [MyTaskbar announcement](https://www.reddit.com/r/SideProject/comments/1vkht9t/mytaskbar_opensource_macos_taskbar_swift_appkit/) cites CPU, sleep, and display recovery as reasons for another implementation.
Those are the author's claims, rather than verified competitor measurements.
[DockDoor's October 2 release notes](https://dockdoor.net/CHANGELOG.html) also describe recovery from stuck Dock/preview clicks after macOS blocks input.
**Inference:** correctness under native permission and input conditions deserves more priority than another visual feature.
This does not establish that Rowla and DockDoor share a root cause.

## Target users and product fit

| Segment | Job to complete | Rowla's relevant behavior | Validation question |
| --- | --- | --- | --- |
| Windows-to-Mac migrants | Find each window from a persistent bar | One window button and Start search | Does the bar replace a repeated Dock workaround? |
| Developers and document-heavy users | Recover a familiar order after many windows open | Explicit app precedence, recent main windows before popups | Does Sort remain useful after the first week? |
| Multi-display users | Find the correct window on the correct screen | Display and Space filters | Do sleep, reconnect, and Space changes preserve usable controls? |
| Users who want inspectable local tools | Understand permissions and resource use | MIT source, local previews, bounded caches | Can they install and build with confidence? |

These are candidate segments, rather than measured Rowla audiences.
The best initial fit is a busy-window user who values a stable layout. Avoid a general productivity claim without measured user results.

## Differentiation to demonstrate

1. Show Sort on an intentionally disordered set of fixture windows.
2. Show direct activation and hover close on normal and minimized windows.
3. Report click and preview latency with the workload and hardware.
4. Report memory and CPU over time for idle, hover, source changes, and sleep recovery.
5. Explain cached preview latency separately from first capture latency.

The cache's 16 MiB / 32-image bound is useful evidence about one allocation category. It does not bound total native process memory.
Do not claim zero idle CPU, zero leaks, or faster performance than competitors without comparable measurements.
A clean short demo from fixture windows remains a useful contribution priority. The QA renders are not a finished product demo.

## Main adoption barriers

- The current beta needs macOS 26+ and Apple Silicon. Several competitors support older macOS versions and Intel.
- First launch needs an Open Anyway exception. DockDoor advertises a notarized download.
- Screen Recording and Accessibility grants need a clear explanation and reliable behavior after updates.
- Established competitors already offer free previews, customization, grouping, or app order.
- A new public project needs independent installation and daily-use results.

Prioritize first-run clarity and cross-application reliability before a broad feature expansion.
Notarization and a Homebrew route are worthwhile future distribution work. They need an owner-approved release process.

## A small demand test

This is a proposed test, not completed research or an active recruitment campaign.
Use a cohort of **20 independent beta users** over **14 days**. Exclude the maintainer's own download and verification activity.
Seek 12 Windows migrants, six busy-window Mac users, and two users interested in inspectable native tools.
These quotas test candidate segments. They do not estimate their share of the market.

Ask each participant to report:

- Whether installation and permissions succeeded.
- Their previous window-navigation method and the specific difficulty.
- Whether they still use Rowla after seven days.
- Whether Sort changes their daily workflow.
- One failure or reason they stopped use.

Use voluntary GitHub feedback. Keep in-app telemetry disabled. Do not collect private window titles or workplace content.
A preliminary continuation threshold is **10 of 20 users still active after seven days**, with repeat use of Sort and a clear preferred workflow.
This threshold is a product decision rule, rather than a forecast or statistical proof.
Investigate installation failures and stop reasons before another feature sprint.

After retention evidence, ask about optional support or a convenient signed distribution. Keep the MIT source available.
Competitor prices suggest questions to ask, rather than a price that Rowla can already charge.

## Discovery routes

The public repository now has download, support, contribution, and agent entry points.
Next useful experiments are a short real demo, a privacy-safe first-run report, and opt-in beta feedback.
Possible later launch channels include r/macapps, Windows-to-Mac communities, native Mac newsletters, and open-source directories.
Observe each community's current rules and disclose authorship. External posts, outreach, and directory submissions need owner authorization.

## Open questions

No independent Rowla retention, paid conversion, competitor revenue, or reliable search-volume estimate is available in this research.
No interviews or outreach occurred. No competitor binaries were installed or benchmarked.
Use direct beta feedback to decide the next product investment.
