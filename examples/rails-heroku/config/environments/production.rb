Rails.application.configure do
  config.eager_load = true
  config.active_storage.service = :local
  config.active_job.queue_adapter = :sidekiq
  config.log_level = :info
end
