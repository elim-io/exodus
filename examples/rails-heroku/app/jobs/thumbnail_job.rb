class ThumbnailJob < ApplicationJob
  queue_as :default

  def perform(photo)
    photo.image.variant(resize_to_limit: [400, 400]).processed
  end
end
