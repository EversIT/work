import type {
  CardOut,
  FieldOut,
  ListDetail,
  StageOut,
  ViewOut,
} from '../../api/generated/model';

export type Member = {
  user_id: string;
  first_name: string;
  last_name: string;
  email: string;
  role: string;
};

export type ViewProps = {
  list: ListDetail;
  stages: StageOut[];
  fields: FieldOut[];
  cards: CardOut[];
  members: Member[];
  view: ViewOut;
  onOpen: (cardId: string) => void;
  onMove: (card: CardOut, stageId: string) => void;
  onQuickCreate: (title: string, stageId?: string) => Promise<void>;
  /** Scrum lists: the running sprint, used to pre-fill new issues. */
  activeSprintId?: string | null;
};

export type ViewOptions = {
  groupBy?: { type: 'list_stage' | 'field' | 'all'; fieldId?: string };
  hideCompleted?: boolean;
  hideChildren?: boolean;
  sortBy?: string; // `order` | `created_at` | `updated_at` | `data.<slug>`
  sortOrder?: 'asc' | 'desc';
  columns?: string[];
};
