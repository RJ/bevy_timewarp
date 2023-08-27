# TimewarpSet::RecordComponentValues

* add_timewarp_buffer_components::<T>
* record_component_added_to_alive_ranges::<T>
* record_component_history_values::<T>
** process_freshly_added_despawn_markers::<T>,

# TimewarpSet::RollbackUnderwayComponents
```
    .run_if(resource_exists::<Rollback>())
    .run_if(not(resource_added::<Rollback>()))
```

* reinsert_components_removed_during_rollback_at_correct_frame::<T>,
* reremove_components_inserted_during_rollback_at_correct_frame::<T>,
** clear_removed_components_queue::<T>

# TimewarpSet::RollbackUnderwayGlobal
```
    .run_if(resource_exists::<Rollback>())
    .run_if(not(resource_added::<Rollback>()))
```

* do_actual_despawn_after_rollback_frames_from_despawn_marker
* check_for_rollback_completion

# TimewarpSet::NoRollback
```
    .run_if(not(resource_exists::<Rollback>()))
    .run_if(not(resource_added::<Rollback>()))
```

* record_component_removed_to_alive_ranges::<T>,
* insert_components_at_prior_frames::<T>,  		 // can req rb
* apply_snapshots_and_rollback_for_non_anachronous::<T>, // can req rb
* apply_snapshots_and_snap_for_anachronous::<T>,
** consolidate_rollback_requests

# TimewarpSet::RollbackInitiated
```
    .run_if(resource_added::<Rollback>())
```

* rollback_initiated
** rollback_initiated_for_component::<T>