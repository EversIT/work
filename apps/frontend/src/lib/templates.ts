/**
 * Starting points for a project's statuses and fields, picked by what the person says about their
 * team ("support team with triage and customer feedback", "bug tracker for a mobile app"). A
 * template library with keywords in English and German, not a model: it ranks the templates by
 * how many of their keywords the description contains, and the person chooses what to add.
 */
export type Loc = { en: string; de: string };
export type Locale = 'en' | 'de';

export type StatusTemplate = {
  id: string;
  name: Loc;
  keywords: string[];
  stages: { name: Loc; color: string; done: boolean }[];
};

export type FieldTemplate = {
  id: string;
  name: Loc;
  keywords: string[];
  fields: { name: Loc; type: string; items?: Loc[] }[];
};

const l = (en: string, de: string = en): Loc => ({ en, de });
const stage = (en: string, de: string, color: string, done = false) => ({
  name: l(en, de),
  color,
  done,
});

export const STATUS_TEMPLATES: StatusTemplate[] = [
  {
    id: 'software',
    name: l('Software development', 'Softwareentwicklung'),
    keywords: [
      'software',
      'development',
      'developer',
      'dev',
      'code',
      'sprint',
      'app',
      'entwicklung',
      'entwickler',
      'programm',
      'scrum',
    ],
    stages: [
      stage('Backlog', 'Backlog', 'grey'),
      stage('To Do', 'Zu erledigen', 'grey'),
      stage('In Progress', 'In Arbeit', 'blue'),
      stage('In Review', 'In Prüfung', 'purple'),
      stage('Done', 'Erledigt', 'green', true),
    ],
  },
  {
    id: 'bugs',
    name: l('Bug tracking', 'Fehlerverfolgung'),
    keywords: [
      'bug',
      'bugs',
      'defect',
      'defects',
      'qa',
      'quality',
      'test',
      'testing',
      'fehler',
      'qualität',
      'tester',
    ],
    stages: [
      stage('New', 'Neu', 'grey'),
      stage('Confirmed', 'Bestätigt', 'orange'),
      stage('In Progress', 'In Arbeit', 'blue'),
      stage('Fixed', 'Behoben', 'teal'),
      stage('Verified', 'Geprüft', 'green', true),
    ],
  },
  {
    id: 'support',
    name: l('Customer support', 'Kundensupport'),
    keywords: [
      'support',
      'service',
      'helpdesk',
      'customer',
      'customers',
      'ticket',
      'tickets',
      'triage',
      'feedback',
      'kunde',
      'kunden',
      'kundenservice',
    ],
    stages: [
      stage('New', 'Neu', 'grey'),
      stage('Triage', 'Sichtung', 'orange'),
      stage('Waiting for customer', 'Wartet auf Kunden', 'purple'),
      stage('In Progress', 'In Arbeit', 'blue'),
      stage('Resolved', 'Gelöst', 'green', true),
    ],
  },
  {
    id: 'marketing',
    name: l('Marketing and content', 'Marketing und Inhalte'),
    keywords: [
      'marketing',
      'campaign',
      'campaigns',
      'content',
      'social',
      'editorial',
      'blog',
      'kampagne',
      'kampagnen',
      'redaktion',
      'inhalte',
    ],
    stages: [
      stage('Idea', 'Idee', 'grey'),
      stage('Planned', 'Geplant', 'purple'),
      stage('In Production', 'In Produktion', 'blue'),
      stage('In Review', 'In Prüfung', 'orange'),
      stage('Published', 'Veröffentlicht', 'green', true),
    ],
  },
  {
    id: 'hiring',
    name: l('Hiring', 'Recruiting'),
    keywords: [
      'hiring',
      'recruiting',
      'recruitment',
      'candidate',
      'candidates',
      'applicant',
      'applicants',
      'bewerber',
      'bewerbung',
      'personal',
      'einstellung',
      'recruiter',
    ],
    stages: [
      stage('Applied', 'Beworben', 'grey'),
      stage('Screening', 'Vorauswahl', 'orange'),
      stage('Interview', 'Gespräch', 'blue'),
      stage('Offer', 'Angebot', 'purple'),
      stage('Hired', 'Eingestellt', 'green', true),
    ],
  },
  {
    id: 'sales',
    name: l('Sales pipeline', 'Vertriebs-Pipeline'),
    keywords: [
      'sales',
      'crm',
      'deal',
      'deals',
      'lead',
      'leads',
      'pipeline',
      'vertrieb',
      'verkauf',
      'akquise',
    ],
    stages: [
      stage('Lead', 'Lead', 'grey'),
      stage('Contacted', 'Kontaktiert', 'blue'),
      stage('Proposal', 'Angebot', 'purple'),
      stage('Negotiation', 'Verhandlung', 'orange'),
      stage('Won', 'Gewonnen', 'green', true),
    ],
  },
  {
    id: 'simple',
    name: l('Simple to-do list', 'Einfache Aufgabenliste'),
    keywords: [],
    stages: [
      stage('To Do', 'Zu erledigen', 'grey'),
      stage('In Progress', 'In Arbeit', 'blue'),
      stage('Done', 'Erledigt', 'green', true),
    ],
  },
];

export const FIELD_TEMPLATES: FieldTemplate[] = [
  {
    id: 'bugs',
    name: l('Bug tracker', 'Fehlerverfolgung'),
    keywords: [
      'bug',
      'bugs',
      'defect',
      'defects',
      'tracker',
      'fehler',
      'qa',
      'test',
    ],
    fields: [
      {
        name: l('Severity', 'Schweregrad'),
        type: 'dropdown',
        items: [
          l('Critical', 'Kritisch'),
          l('Major', 'Hoch'),
          l('Minor', 'Gering'),
          l('Trivial', 'Trivial'),
        ],
      },
      { name: l('Environment', 'Umgebung'), type: 'text' },
      {
        name: l('Steps to reproduce', 'Schritte zum Nachstellen'),
        type: 'text_area',
      },
    ],
  },
  {
    id: 'mobile',
    name: l('Mobile app', 'Mobile App'),
    keywords: ['mobile', 'app', 'ios', 'android', 'smartphone', 'handy'],
    fields: [
      {
        name: l('Platform', 'Plattform'),
        type: 'dropdown',
        items: [l('iOS'), l('Android'), l('Web')],
      },
      { name: l('App version', 'App-Version'), type: 'text' },
      { name: l('Device', 'Gerät'), type: 'text' },
    ],
  },
  {
    id: 'web',
    name: l('Website or web app', 'Website oder Web-App'),
    keywords: [
      'web',
      'website',
      'webapp',
      'browser',
      'frontend',
      'webseite',
      'portal',
    ],
    fields: [
      {
        name: l('Browser'),
        type: 'dropdown',
        items: [l('Chrome'), l('Firefox'), l('Safari'), l('Edge')],
      },
      { name: l('Page address', 'Seitenadresse'), type: 'url' },
      {
        name: l('Environment', 'Umgebung'),
        type: 'dropdown',
        items: [
          l('Production', 'Produktion'),
          l('Staging'),
          l('Development', 'Entwicklung'),
        ],
      },
    ],
  },
  {
    id: 'support',
    name: l('Customer support', 'Kundensupport'),
    keywords: [
      'support',
      'helpdesk',
      'customer',
      'customers',
      'service',
      'kunde',
      'kunden',
      'ticket',
    ],
    fields: [
      { name: l('Customer', 'Kunde'), type: 'text' },
      {
        name: l('Channel', 'Kanal'),
        type: 'dropdown',
        items: [
          l('Email', 'E-Mail'),
          l('Phone', 'Telefon'),
          l('Chat'),
          l('Web form', 'Webformular'),
        ],
      },
      {
        name: l('Urgency', 'Dringlichkeit'),
        type: 'dropdown',
        items: [l('Low', 'Niedrig'), l('Normal'), l('High', 'Hoch')],
      },
    ],
  },
  {
    id: 'sales',
    name: l('Sales and CRM', 'Vertrieb und CRM'),
    keywords: [
      'sales',
      'crm',
      'deal',
      'deals',
      'lead',
      'leads',
      'vertrieb',
      'verkauf',
    ],
    fields: [
      { name: l('Company', 'Firma'), type: 'text' },
      { name: l('Deal value', 'Auftragswert'), type: 'currency' },
      {
        name: l('Source', 'Quelle'),
        type: 'dropdown',
        items: [
          l('Website'),
          l('Referral', 'Empfehlung'),
          l('Event', 'Veranstaltung'),
          l('Cold call', 'Kaltakquise'),
        ],
      },
    ],
  },
  {
    id: 'hiring',
    name: l('Hiring', 'Recruiting'),
    keywords: [
      'hiring',
      'recruiting',
      'recruitment',
      'candidate',
      'candidates',
      'bewerber',
      'personal',
      'einstellung',
    ],
    fields: [
      { name: l('Position', 'Stelle'), type: 'text' },
      {
        name: l('Source', 'Quelle'),
        type: 'dropdown',
        items: [
          l('LinkedIn'),
          l('Referral', 'Empfehlung'),
          l('Website'),
          l('Agency', 'Agentur'),
        ],
      },
      { name: l('Expected salary', 'Gehaltswunsch'), type: 'currency' },
    ],
  },
  {
    id: 'marketing',
    name: l('Marketing campaign', 'Marketing-Kampagne'),
    keywords: [
      'marketing',
      'campaign',
      'campaigns',
      'content',
      'social',
      'kampagne',
      'kampagnen',
    ],
    fields: [
      {
        name: l('Channel', 'Kanal'),
        type: 'dropdown',
        items: [
          l('Social'),
          l('Email', 'E-Mail'),
          l('Blog'),
          l('Event', 'Veranstaltung'),
          l('Paid', 'Bezahlt'),
        ],
      },
      { name: l('Budget'), type: 'currency' },
      { name: l('Audience', 'Zielgruppe'), type: 'text' },
    ],
  },
];

const words = (text: string) =>
  text.toLowerCase().match(/[\p{L}\p{N}]+/gu) ?? [];

/**
 * The templates that fit `text`, best first. A template needs at least one keyword in the text; when
 * none fits, `fallback` ids are returned so the person always gets something to start from.
 */
export function rankTemplates<T extends { id: string; keywords: string[] }>(
  all: T[],
  text: string,
  fallback: string[] = [],
  limit = 3,
): T[] {
  const w = new Set(words(text));
  const scored = all
    .map((t) => ({ t, hits: t.keywords.filter((k) => w.has(k)).length }))
    .filter((x) => x.hits > 0)
    .sort((a, b) => b.hits - a.hits);
  if (scored.length === 0)
    return all.filter((t) => fallback.includes(t.id)).slice(0, limit);
  return scored.slice(0, limit).map((x) => x.t);
}

export const label = (l: Loc, locale: string) =>
  locale === 'de' ? l.de : l.en;
