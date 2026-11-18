# SOFTWARE REQUIREMENTS SPECIFICATION (SRS)
# FocusTrack – Windows Screen Time & Digital Wellbeing Application

**Document Version:** 1.0  
**Product Name:** FocusTrack  
**Product Type:** Windows Desktop Application  
**Target Platforms:** Windows 10 and Windows 11  
**Document Status:** Initial Product Requirements

---

# 1. PRODUCT OVERVIEW

## 1.1 Introduction

FocusTrack is a Windows desktop application designed to monitor, record, analyse and manage how users spend time on their computers.

The application will provide functionality similar to Apple's Screen Time and Android's Digital Wellbeing.

It will automatically track application usage, website visits, active computer time and idle time. Users will be able to view detailed usage statistics, understand their digital habits, set screen time limits and improve their productivity.

## 1.2 Product Vision

To create an intelligent digital wellbeing application that helps users understand and manage their computer usage through automatic tracking, meaningful analytics and personalised productivity tools.

## 1.3 Primary Objectives

- Automatically track computer usage.
- Monitor time spent in individual applications.
- Track time spent on individual websites.
- Distinguish active usage from idle time.
- Generate daily, weekly and monthly reports.
- Help users identify time-consuming activities.
- Allow users to establish application and website usage limits.
- Provide productivity and focus management features.
- Protect user privacy by keeping tracking data local by default.

## 1.4 Target Audience

- Students and learners.
- Working professionals.
- Remote workers.
- Freelancers.
- People interested in improving productivity.
- Individuals looking to reduce excessive screen time.
- Families requiring optional parental monitoring features.

---

# 2. CORE APPLICATION FEATURES

## 2.1 Dashboard

The dashboard will be the application's main screen.

It should provide an overview of the user's computer usage.

### Functional Requirements

- Display total screen time for the current day.
- Display total active computer time.
- Display total idle time.
- Show screen time compared with the previous day.
- Display daily average usage.
- Display the most-used applications.
- Display the most-used websites.
- Display the number of computer usage sessions.
- Display a weekly screen time chart.
- Display application category breakdowns.
- Display productivity statistics.
- Show current usage session information.

### Dashboard Metrics

- Total screen time.
- Active time.
- Idle time.
- Application usage.
- Website usage.
- Daily average.
- Weekly average.
- Most-used application.
- Most-used website.
- Usage trends.

---

## 2.2 Application Usage Tracking

The application must automatically detect and track applications being actively used on the computer.

### Functional Requirements

- Detect the currently active foreground application.
- Identify application names and executable processes.
- Record application start and end times.
- Calculate time spent in each application.
- Track application usage throughout the day.
- Maintain historical application usage records.
- Display application usage statistics.
- Identify the most frequently used applications.
- Support application categorisation.
- Allow users to customise application categories.

### Application Categories

- Productivity.
- Education.
- Entertainment.
- Social Media.
- Gaming.
- Communication.
- Development.
- Other.

### Example Application Usage Report

| Application | Time Spent |
|---|---|
| Google Chrome | 2h 15m |
| Visual Studio Code | 1h 45m |
| Microsoft Word | 55m |
| Spotify | 40m |
| Discord | 35m |
| Steam | 1h 10m |

---

## 2.3 Website Usage Tracking

The application should track how much time users spend on individual websites.

Website tracking will be implemented using browser extensions.

### Supported Browsers

- Google Chrome.
- Microsoft Edge.
- Mozilla Firefox.

### Functional Requirements

- Detect active browser tabs.
- Identify website domains.
- Record website usage duration.
- Track website usage sessions.
- Calculate daily website usage.
- Display the most-used websites.
- Maintain website usage history.
- Categorise websites.
- Allow manual website categorisation.
- Display website usage trends.

### Example Website Usage Report

| Website | Time Spent |
|---|---|
| YouTube | 1h 20m |
| Instagram | 45m |
| Reddit | 35m |
| GitHub | 1h 10m |
| LinkedIn | 25m |

The application should track website domains rather than collect private browsing content.

---

## 2.4 Screen Time Reports and Analytics

Users should be able to access detailed reports about their computer usage.

### Available Report Periods

- Today.
- Yesterday.
- Last 7 days.
- Last 30 days.
- Custom date range.

### Report Information

- Total screen time.
- Total active time.
- Total idle time.
- Daily average usage.
- Application usage breakdown.
- Website usage breakdown.
- Category-based usage.
- Most-used applications.
- Most-used websites.
- Most active hours.
- Weekday versus weekend comparison.
- Historical usage trends.
- Comparison with previous periods.

### Visualisations

- Daily usage bar charts.
- Weekly usage charts.
- Monthly usage charts.
- Application usage pie charts.
- Website usage breakdowns.
- Activity heatmaps.
- Usage trend graphs.

---

## 2.5 Application Usage Limits

Users should be able to configure daily usage limits for individual applications.

### Functional Requirements

- Set daily limits for applications.
- Edit or remove existing limits.
- Display remaining usage time.
- Notify users when approaching a limit.
- Notify users when a limit is exceeded.
- Provide configurable warning thresholds.
- Optionally restrict application usage after reaching a limit.

### Example

| Application | Daily Limit |
|---|---|
| YouTube | 1 hour |
| Instagram | 30 minutes |
| Steam | 2 hours |
| Discord | 1 hour |

The initial implementation should prioritise notifications and warnings. Application blocking can be introduced as an optional feature.

---

## 2.6 Website Usage Limits

Users should be able to establish daily limits for specific websites.

### Functional Requirements

- Set daily website usage limits.
- Configure limits for individual websites.
- Display remaining website usage time.
- Notify users when approaching limits.
- Notify users when limits are exceeded.
- Optionally block selected websites through browser extensions.

---

## 2.7 Focus Mode

Focus Mode will help users concentrate on specific activities.

### Functional Requirements

- Start and stop focus sessions.
- Configure custom focus durations.
- Provide predefined focus sessions.
- Support Pomodoro sessions.
- Configure break durations.
- Record completed focus sessions.
- Display focus session history.
- Display total focused time.
- Optionally restrict distracting websites.
- Provide focus session notifications.

### Predefined Focus Sessions

- 25-minute Pomodoro.
- 50-minute deep work.
- 90-minute study session.
- Custom duration.

---

## 2.8 Idle Time Detection

The application must distinguish between active computer usage and inactivity.

### Functional Requirements

- Detect keyboard and mouse inactivity.
- Identify when the computer becomes idle.
- Stop counting active usage after a configurable inactivity threshold.
- Detect Windows lock events.
- Detect sleep and hibernation.
- Resume tracking when activity returns.
- Exclude idle periods from active usage statistics.

The application should use Windows APIs to detect idle time rather than recording keyboard inputs or keystrokes.

---

## 2.9 Notifications and Reminders

The application should provide configurable notifications about user activity.

### Example Notifications

- You have spent 2 hours on YouTube today.
- You have reached 80% of your Instagram limit.
- You have been using your laptop for 3 hours.
- You have completed a 50-minute focus session.
- Your screen time increased by 20% compared with yesterday.
- You have exceeded your daily screen time goal.

### Functional Requirements

- Windows desktop notifications.
- Customisable notification thresholds.
- Enable or disable individual notification types.
- Configurable notification preferences.
- Usage limit notifications.
- Focus session notifications.

---

## 2.10 Historical Usage Records

Users should be able to access previously recorded usage information.

### Functional Requirements

- View daily usage history.
- View weekly usage history.
- View monthly usage history.
- View application usage history.
- View website usage history.
- Search historical records.
- Filter by application.
- Filter by website.
- Filter by date.
- Export usage reports to CSV.

---

# 3. ADDITIONAL FEATURES

These features can be considered for future development.

## 3.1 AI-Powered Insights

The application could analyse usage patterns and generate personalised observations.

Examples:

- Your YouTube usage increased by 35% this week.
- You spend most of your computer time between 2 PM and 6 PM.
- Your productivity application usage increased this month.
- You spent 8 hours on entertainment applications this week.

## 3.2 Daily Goals

Allow users to establish personal digital wellbeing goals.

Examples:

- Limit total screen time to 6 hours.
- Spend at least 2 hours studying.
- Limit social media usage to 30 minutes.
- Complete 3 focus sessions daily.

## 3.3 Achievements and Streaks

Introduce a gamification system to encourage positive habits.

Examples:

- Complete 5 focus sessions.
- Stay below your daily screen time limit.
- Maintain a 7-day productivity streak.
- Reduce entertainment usage by 20%.

## 3.4 Activity Heatmap

Display computer activity throughout the day and week.

The heatmap should help users identify:

- Most active hours.
- Least active hours.
- Peak computer usage periods.
- Daily activity patterns.

## 3.5 Privacy Controls

Users should have full control over their tracking data.

Features:

- Pause tracking.
- Resume tracking.
- Exclude selected applications.
- Exclude selected websites.
- Delete historical data.
- Clear all usage records.
- Configure data retention.
- Enable or disable website tracking.

---

# 4. APPLICATION NAVIGATION AND SCREEN STRUCTURE

The application should contain the following primary navigation sections:

1. Dashboard
2. Applications
3. Websites
4. Reports
5. Focus Mode
6. Usage Limits
7. History
8. Settings

## 4.1 Dashboard

Overview of daily computer usage and important statistics.

## 4.2 Applications

Detailed application usage, categories and historical statistics.

## 4.3 Websites

Website usage information, website categories and website statistics.

## 4.4 Reports

Daily, weekly and monthly analytics.

## 4.5 Focus Mode

Focus timers, sessions and productivity tracking.

## 4.6 Usage Limits

Application and website usage limits.

## 4.7 History

Historical application and website usage records.

## 4.8 Settings

General settings, tracking preferences, privacy settings, notifications and appearance.

---

# 5. FUNCTIONAL REQUIREMENTS SUMMARY

| ID | Requirement | Priority |
|---|---|---|
| FR-01 | Automatically track computer usage | Critical |
| FR-02 | Detect active applications | Critical |
| FR-03 | Calculate application usage duration | Critical |
| FR-04 | Detect idle time | Critical |
| FR-05 | Display daily usage dashboard | Critical |
| FR-06 | Store historical usage data | Critical |
| FR-07 | Generate weekly and monthly reports | High |
| FR-08 | Track individual website usage | High |
| FR-09 | Categorise applications | High |
| FR-10 | Configure application usage limits | High |
| FR-11 | Generate notifications | High |
| FR-12 | Provide focus mode | Medium |
| FR-13 | Export usage reports | Medium |
| FR-14 | Generate AI insights | Future |
| FR-15 | Provide achievements and streaks | Future |

---

# 6. NON-FUNCTIONAL REQUIREMENTS

## 6.1 Performance

- The application must run efficiently in the background.
- It should consume minimal CPU and RAM.
- Tracking must not noticeably affect computer performance.
- Dashboard statistics should load quickly.
- Usage statistics should update without noticeable delay.

## 6.2 Reliability

- Support automatic startup with Windows when enabled.
- Recover tracking correctly after a system restart.
- Prevent duplicate usage records.
- Handle sleep and hibernation correctly.
- Preserve historical records.
- Handle unexpected application shutdowns.

## 6.3 Security and Privacy

- Store user data locally by default.
- Do not record keyboard keystrokes.
- Do not capture screenshots.
- Do not collect passwords.
- Do not collect private browsing content.
- Allow users to pause tracking.
- Allow users to delete their tracking history.
- Encrypt sensitive information where appropriate.
- Provide transparent tracking controls.

## 6.4 Usability

- Simple and intuitive interface.
- Modern desktop application design.
- Light and dark themes.
- Easy-to-understand analytics.
- Responsive interface.
- Accessible controls.
- Support Windows display scaling.

---

# 7. TECHNICAL ARCHITECTURE

## 7.1 Recommended Technology Stack

| Component | Technology |
|---|---|
| Desktop Framework | Tauri 2 |
| Frontend | React |
| Frontend Language | TypeScript |
| Backend | Rust |
| Database | SQLite |
| UI Styling | Tailwind CSS |
| Charts | Recharts |
| Windows Integration | Windows APIs |
| Browser Integration | Browser Extensions |

## 7.2 Architecture Components

### Frontend

Responsible for:

- Dashboard.
- Application usage screens.
- Website usage screens.
- Reports and charts.
- Focus Mode.
- Settings.
- Notifications interface.

### Backend

Responsible for:

- Application tracking.
- Active window detection.
- Idle time detection.
- Usage calculations.
- Tracking session management.
- Windows integration.
- Communication with the database.

### Database

Responsible for storing:

- Application information.
- Application usage sessions.
- Website information.
- Website usage sessions.
- Categories.
- Usage limits.
- Focus sessions.
- Daily summaries.
- User settings.

### Browser Extensions

Responsible for:

- Detecting active browser tabs.
- Identifying website domains.
- Recording website activity duration.
- Communicating website usage information to the desktop application.

---

# 8. DATABASE DESIGN

The application should use SQLite for local data storage.

Proposed database tables:

| Table | Description |
|---|---|
| applications | Application information |
| usage_sessions | Application usage sessions |
| websites | Website information |
| website_sessions | Website usage sessions |
| categories | Application and website categories |
| usage_limits | Configured usage limits |
| focus_sessions | Focus session records |
| daily_summaries | Aggregated daily statistics |
| user_settings | Application preferences |

## 8.1 Example Application Usage Record

```json
{
  "application": "Google Chrome",
  "start_time": "2026-11-18T09:00:00",
  "end_time": "2026-11-18T10:15:00",
  "duration_minutes": 75,
  "category": "Productivity"
}
```

---

# 9. MVP – MINIMUM VIABLE PRODUCT

The first release should focus on the essential functionality required to deliver a working screen time tracking application.

## Phase 1: Core Tracking

- Automatic computer usage tracking.
- Active application detection.
- Application identification.
- Idle time detection.
- Usage session recording.
- Local SQLite database.
- Windows startup integration.

## Phase 2: Dashboard and Analytics

- Main dashboard.
- Total daily screen time.
- Active and idle time.
- Application usage breakdown.
- Application categories.
- Daily statistics.
- Weekly statistics.
- Usage charts.

## Phase 3: Advanced Tracking

- Browser extensions.
- Website tracking.
- Website usage analytics.
- Application usage limits.
- Website usage limits.
- Desktop notifications.

## Phase 4: Productivity Features

- Focus Mode.
- Pomodoro timer.
- Daily goals.
- Productivity reports.
- Activity heatmap.
- Usage insights.

## Phase 5: Advanced Features

- AI-powered insights.
- Achievements and streaks.
- Cloud synchronisation.
- Cross-device support.
- Optional family features.

---

# 10. SUCCESS CRITERIA

The MVP will be considered successful when:

1. The application accurately tracks active foreground application usage.
2. The application distinguishes active time from idle time.
3. Usage records are saved reliably.
4. Daily and weekly statistics are calculated correctly.
5. The application handles sleep, restart and shutdown correctly.
6. The application runs reliably in the background.
7. The application has minimal CPU and memory consumption.
8. Users can view application usage statistics.
9. Users can pause tracking.
10. Users can delete their historical usage data.

---

# 11. FUTURE EXPANSION

- Mobile companion application.
- Cloud synchronisation.
- Cross-device usage tracking.
- AI productivity coach.
- Advanced focus scheduling.
- Personalised productivity recommendations.
- Multi-user family accounts.
- Optional web-based analytics dashboard.

---

# 12. FINAL PRODUCT DIRECTION

FocusTrack should be developed as an offline-first Windows desktop application.

The initial priority should be building a reliable background tracking engine capable of accurately identifying active applications, calculating usage duration and distinguishing active time from idle time.

Once the tracking engine is reliable, the next priority should be developing a modern analytics dashboard.

Website tracking, application limits, focus mode and AI-powered insights should be introduced incrementally.

The overall goal is to create a complete digital wellbeing platform for Windows rather than simply an application that displays how long a computer has been switched on.

**Recommended MVP Technology Stack:** Tauri 2 + React + TypeScript + Rust + SQLite.

**Primary Product Goal:** Help users understand, monitor and improve how they spend their time on their Windows computers.
