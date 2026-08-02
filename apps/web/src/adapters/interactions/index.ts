/**
 * Cross-module data-interaction barrel.
 *
 * Re-exports the {@link EventBus} and the four interaction controllers:
 *  - Content ↔ Knowledge (5.4.1)
 *  - GTD ↔ Content       (5.4.2)
 *  - AI ↔ Content ↔ Knowledge (5.4.3)
 *  - Sync cross-cutting  (5.4.4)
 */

export {
  EventBus,
  sharedEventBus,
  type Subscription,
  type EventBusOptions,
  type CoreEventType,
  type TypedEventHandler,
} from './eventBus';

export {
  ContentKnowledgeController,
  InMemoryKnowledgeLinkStore,
  parseWikiLinks,
  parseMarkdownLinks,
  useContentKnowledgeLink,
  type KnowledgeLinkStore,
  type MarkdownLink,
  type ParsedLinks,
  type ContentKnowledgeControllerOptions,
  type UseContentKnowledgeLink,
} from './contentKnowledgeInteraction';

export {
  GtdContentController,
  InMemoryContentBlockStore,
  useGtdContentLink,
  DEFAULT_DOC_ID,
  TASK_BLOCK_TYPE,
  type ContentBlockStore,
  type TaskBlockRecord,
  type GtdContentControllerOptions,
  type UseGtdContentLink,
} from './gtdContentInteraction';

export {
  AiInteractionController,
  InMemoryAiOutputStore,
  InMemoryHighlightStore,
  extractDocReferences,
  useAiInteraction,
  type AiOutputStore,
  type HighlightStore,
  type SemanticSearchRequest,
  type AiInteractionControllerOptions,
  type UseAiInteraction,
} from './aiContentKnowledgeInteraction';

export {
  SyncOrchestrator,
  InMemorySyncQueue,
  MockSyncDispatcher,
  useSyncOrchestrator,
  type SyncQueueItem,
  type SyncQueueStore,
  type SyncTargetDispatcher,
  type DispatchStatus,
  type SyncOrchestratorOptions,
  type UseSyncOrchestrator,
} from './syncCrossCutting';
