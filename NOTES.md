# TimewarpSet::RecordComponentValues

* add_frame_to_freshly_added_despawn_markers

* add_timewarp_buffer_components::<T>
* record_component_added_to_alive_ranges::<T>
* record_component_history_values::<T>
** remove_components_from_entities_with_freshly_added_despawn_markers::<T>,

# TimewarpSet::RollbackUnderwayComponents
```
    .run_if(resource_exists::<Rollback>())
    .run_if(not(resource_added::<Rollback>()))
```

* apply_snapshot_to_component_if_available::<T>, 
* reinsert_components_removed_during_rollback_at_correct_frame::<T>,
* reremove_components_inserted_during_rollback_at_correct_frame::<T>,
** clear_removed_components_queue::<T>

# TimewarpSet::RollbackUnderwayGlobal
```
    .run_if(resource_exists::<Rollback>())
    .run_if(not(resource_added::<Rollback>()))
```

* check_for_rollback_completion

# TimewarpSet::NoRollback
```
    .run_if(not(resource_exists::<Rollback>()))
    .run_if(not(resource_added::<Rollback>()))
```

* record_component_removed_to_alive_ranges::<T>,
* insert_components_at_prior_frames::<T>,  		 
* apply_snapshot_to_component_if_available::<T>, // can req rb
* trigger_rollback_when_snapshot_added<T> (after insert_comp_at_priors) // can req rb
** consolidate_rollback_requests
*** do_actual_despawn_after_rollback_frames_from_despawn_marker

# TimewarpSet::RollbackInitiated
```
    .run_if(resource_added::<Rollback>())
```

* rollback_initiated
** rollback_initiated_for_component::<T>