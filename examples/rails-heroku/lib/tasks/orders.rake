# Run nightly by Heroku Scheduler: rake orders:sync
namespace :orders do
  task sync: :environment do
    Order.where(synced_at: nil).find_each(&:sync!)
  end
end
