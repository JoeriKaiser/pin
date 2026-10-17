import { els } from './dom.js';
import { state, updateShown } from './state.js';
import { stripAnsi, cleanThoughtText, escapeHtml } from './formatters.js';
import {
  setReaderTab, renderActions, renderHandoff, renderList
} from './detail.js';
import {
  openSpecModal, closeSpecModal, setSpecModalTab, SPEC_SNIPPETS,
  expandQuickAdd, collapseQuickAdd, triggerModalSubmit,
  openWorktreeModal, closeWorktreeModal
} from './forms.js';
import {
  getClipboardImages, uploadScreenshot, handleScreenshotPaste
} from './media.js';
import { createItem } from './api.js';
import {
  openAgentDrawer, closeAgentDrawer, handleAgentStreamEvent,
  formatToolCallInline, renderSwimlaneSvg, openToolInspector,
  closeToolInspector, appendTrajectoryContextRow, filterTrajectoryRows,
  updateActiveRuns
} from './agent.js';
import {
  buildBoardCard, handleCardDrop, setViewMode, renderBoard, computeRenderKey
} from './board.js';
import {
  bind, sendRunItem, sendCommitPrItem, handleUpdateInstall, handleGitSync
} from './bind.js';
import { ingest, checkUpdate, connectEvents, refreshData } from './boot.js';
import { setRoute } from './router.js';
import { renderAgentsTable, formatDuration, updateAgentDurations } from './agents-view.js';

if (typeof window !== 'undefined') {
  window._test = {
    els: els,
    state: state,
    setReaderTab: setReaderTab,
    formatToolCallInline: formatToolCallInline,
    renderSwimlaneSvg: renderSwimlaneSvg,
    openToolInspector: openToolInspector,
    closeToolInspector: closeToolInspector,
    handleAgentStreamEvent: handleAgentStreamEvent,
    openAgentDrawer: openAgentDrawer,
    closeAgentDrawer: closeAgentDrawer,
    appendTrajectoryContextRow: appendTrajectoryContextRow,
    filterTrajectoryRows: filterTrajectoryRows,
    cleanThoughtText: cleanThoughtText,
    stripAnsi: stripAnsi,
    bind: bind,
    submitActionDialog: triggerModalSubmit,
    escapeHtml: escapeHtml,
    openSpecModal: openSpecModal,
    closeSpecModal: closeSpecModal,
    setSpecModalTab: setSpecModalTab,
    createItem: createItem,
    SPEC_SNIPPETS: SPEC_SNIPPETS,
    expandQuickAdd: expandQuickAdd,
    collapseQuickAdd: collapseQuickAdd,
    getClipboardImages: getClipboardImages,
    uploadScreenshot: uploadScreenshot,
    handleScreenshotPaste: handleScreenshotPaste,
    openWorktreeModal: openWorktreeModal,
    closeWorktreeModal: closeWorktreeModal,
    sendCommitPrItem: sendCommitPrItem,
    sendRunItem: sendRunItem,
    buildBoardCard: buildBoardCard,
    renderActions: renderActions,
    renderList: renderList,
    renderHandoff: renderHandoff,
    handleCardDrop: handleCardDrop,
    setViewMode: setViewMode,
    renderBoard: renderBoard,
    updateActiveRuns: updateActiveRuns,
    checkUpdate: checkUpdate,
    handleUpdateInstall: handleUpdateInstall,
    handleGitSync: handleGitSync,
    ingest: ingest,
    updateShown: updateShown,
    connectEvents: connectEvents,
    refreshData: refreshData,
    setRoute: setRoute,
    computeRenderKey: computeRenderKey,
    renderAgentsTable: renderAgentsTable,
    formatDuration: formatDuration,
    updateAgentDurations: updateAgentDurations,
  };
}
