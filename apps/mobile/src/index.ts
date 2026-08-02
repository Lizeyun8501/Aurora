/**
 * Aurora Mobile (Capacitor) barrel.
 */

export {
  CapacitorPlatform,
  capacitorPlatform,
} from './adapters/capacitorPlatform';
export {
  CapacitorBridge,
  capacitorBridge,
  MockBridgeBackend,
  initCapacitorBridge,
  type BridgeCall,
  type BridgeBackend,
  type BridgeArgs,
  type RecordedBridgeCall,
} from './adapters/bridge';
