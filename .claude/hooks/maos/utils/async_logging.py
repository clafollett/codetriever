#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# ///

import asyncio
import json
import time
from pathlib import Path
from typing import Dict, Any
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime

class AsyncJSONLLogger:
    """
    High-performance async JSONL logger for hooks.
    
    Features:
    - True append-only JSONL format (no read-modify-write)
    - Atomic writes per line
    - Background processing queue
    - No file locking overhead
    - Batched writes for efficiency
    """
    
    def __init__(self, max_workers: int = 2, batch_size: int = 10):
        self.executor = ThreadPoolExecutor(max_workers=max_workers)
        self.batch_size = batch_size
        self._write_queue = asyncio.Queue()
        self._batch_buffer = []
        self._batch_task = None
        self._shutdown = False
    
    async def start(self):
        """Start the background batch processor."""
        if self._batch_task is None:
            self._batch_task = asyncio.create_task(self._batch_processor())
    
    async def stop(self):
        """Stop the background processor and flush remaining entries."""
        self._shutdown = True
        if self._batch_task:
            await self._batch_task
        
        # Flush any remaining entries
        if self._batch_buffer:
            await self._flush_batch()
        
        self.executor.shutdown(wait=True)
    
    async def log_async(self, log_file: Path, data: Dict[Any, Any]) -> None:
        """
        Log data to JSONL file asynchronously.
        
        Args:
            log_file: Path to JSONL log file
            data: Data to log (will be JSON serialized)
        """
        # Ensure logger is started
        if self._batch_task is None:
            await self.start()
        
        # Add timestamp for better debugging
        log_entry = {
            "timestamp": datetime.utcnow().isoformat(),
            **data
        }
        
        # Add to queue for background processing
        await self._write_queue.put((log_file, log_entry))
    
    def log_sync(self, log_file: Path, data: Dict[Any, Any]) -> None:
        """
        Synchronous fallback for logging.
        Direct write without batching for simple cases.
        """
        try:
            # Ensure directory exists
            log_file.parent.mkdir(parents=True, exist_ok=True)
            
            # Add timestamp
            log_entry = {
                "timestamp": datetime.utcnow().isoformat(),
                **data
            }
            
            # Append single line to JSONL file
            with open(log_file, 'a', encoding='utf-8') as f:
                f.write(json.dumps(log_entry, separators=(',', ':')) + '\n')
        
        except Exception:
            # Fail silently for logging
            pass
    
    async def _batch_processor(self):
        """Background task to process log entries in batches."""
        while not self._shutdown:
            try:
                # Wait for entries or timeout
                try:
                    log_file, entry = await asyncio.wait_for(
                        self._write_queue.get(), 
                        timeout=1.0
                    )
                    
                    # Add to batch buffer
                    self._batch_buffer.append((log_file, entry))
                    
                    # Flush if batch is full
                    if len(self._batch_buffer) >= self.batch_size:
                        await self._flush_batch()
                        
                except asyncio.TimeoutError:
                    # Timeout - flush any pending entries
                    if self._batch_buffer:
                        await self._flush_batch()
            
            except Exception:
                # Log processor should never crash
                pass
    
    async def _flush_batch(self):
        """Flush current batch to files."""
        if not self._batch_buffer:
            return
        
        # Group entries by file for efficient batching
        file_groups = {}
        for log_file, entry in self._batch_buffer:
            if log_file not in file_groups:
                file_groups[log_file] = []
            file_groups[log_file].append(entry)
        
        # Write each file group in executor
        loop = asyncio.get_event_loop()
        tasks = []
        
        for log_file, entries in file_groups.items():
            task = loop.run_in_executor(
                self.executor, 
                self._write_entries_sync, 
                log_file, 
                entries
            )
            tasks.append(task)
        
        # Wait for all writes to complete
        if tasks:
            await asyncio.gather(*tasks, return_exceptions=True)
        
        # Clear buffer
        self._batch_buffer.clear()
    
    def _write_entries_sync(self, log_file: Path, entries: list) -> None:
        """Synchronously write entries to JSONL file."""
        try:
            # Ensure directory exists
            log_file.parent.mkdir(parents=True, exist_ok=True)
            
            # Batch write all entries as JSONL
            with open(log_file, 'a', encoding='utf-8') as f:
                for entry in entries:
                    f.write(json.dumps(entry, separators=(',', ':')) + '\n')
        
        except Exception:
            # Fail silently for logging
            pass


class BackgroundTaskManager:
    """
    Manager for non-blocking background tasks in hooks.
    
    Handles tasks like:
    - MAOS orchestration
    - Rust formatting
    - File processing
    """
    
    def __init__(self, max_workers: int = 4):
        self.executor = ThreadPoolExecutor(max_workers=max_workers)
        self._tasks = []
    
    async def run_background_task(self, func, *args, timeout: float = 30.0) -> None:
        """
        Run a function in background without blocking.
        
        Args:
            func: Function to run
            *args: Arguments for function
            timeout: Maximum time to wait (default: 30 seconds)
        """
        try:
            loop = asyncio.get_event_loop()
            task = loop.run_in_executor(self.executor, func, *args)
            self._tasks.append(task)
            
            # Fire and forget with timeout
            await asyncio.wait_for(task, timeout=timeout)
            
        except asyncio.TimeoutError:
            # Don't block if background task takes too long
            pass
        except Exception:
            # Don't block on background task errors
            pass
        finally:
            # Clean up completed tasks
            self._tasks = [t for t in self._tasks if not t.done()]
    
    def run_fire_and_forget(self, func, *args) -> None:
        """
        Run a function in background and completely forget about it.
        Use for truly non-critical tasks.
        """
        try:
            self.executor.submit(func, *args)
        except Exception:
            # Even submission failures shouldn't block
            pass
    
    async def shutdown(self, timeout: float = 5.0) -> None:
        """Shutdown background tasks gracefully."""
        # Wait for pending tasks to complete (with timeout)
        if self._tasks:
            try:
                await asyncio.wait_for(
                    asyncio.gather(*self._tasks, return_exceptions=True),
                    timeout=timeout
                )
            except asyncio.TimeoutError:
                pass
        
        # Shutdown executor
        self.executor.shutdown(wait=True)


# Global instances for hook system
_logger = None
_task_manager = None

def get_async_logger() -> AsyncJSONLLogger:
    """Get the global async logger instance."""
    global _logger
    if _logger is None:
        _logger = AsyncJSONLLogger(max_workers=2, batch_size=5)
    return _logger

def get_task_manager() -> BackgroundTaskManager:
    """Get the global background task manager."""
    global _task_manager
    if _task_manager is None:
        _task_manager = BackgroundTaskManager(max_workers=4)
    return _task_manager

async def log_hook_data(log_file: Path, data: Dict[Any, Any]) -> None:
    """Convenience function for async hook logging."""
    logger = get_async_logger()
    await logger.log_async(log_file, data)

def log_hook_data_sync(log_file: Path, data: Dict[Any, Any]) -> None:
    """Convenience function for sync hook logging."""
    logger = get_async_logger()
    logger.log_sync(log_file, data)

async def cleanup_async_systems():
    """Clean up global async systems."""
    global _logger, _task_manager
    
    if _logger:
        await _logger.stop()
        _logger = None
    
    if _task_manager:
        await _task_manager.shutdown()
        _task_manager = None


if __name__ == "__main__":
    # Test the async logging system
    import tempfile
    
    async def test_async_logging():
        print("🧪 Testing async JSONL logging...")
        
        with tempfile.TemporaryDirectory() as temp_dir:
            log_file = Path(temp_dir) / "test.jsonl"
            
            # Test async logging
            for i in range(10):
                await log_hook_data(log_file, {
                    "test_entry": i,
                    "message": f"Test log entry {i}"
                })
            
            # Wait for batch processing
            await asyncio.sleep(2)
            
            # Clean up
            await cleanup_async_systems()
            
            # Verify results
            if log_file.exists():
                with open(log_file, 'r') as f:
                    lines = f.readlines()
                print(f"✅ Successfully wrote {len(lines)} JSONL entries")
                
                # Parse first line to verify format
                if lines:
                    entry = json.loads(lines[0])
                    print(f"📋 Sample entry: {entry}")
            else:
                print("❌ Log file not created")
    
    # Run test
    print("Testing AsyncJSONLLogger...")
    asyncio.run(test_async_logging())
    print("✅ Test complete!")