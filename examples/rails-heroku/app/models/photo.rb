class Photo < ApplicationRecord
  has_one_attached :image

  after_create_commit { ThumbnailJob.perform_later(self) }
end
