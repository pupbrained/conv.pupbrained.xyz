<template>
  <div>
    <form @submit.prevent="submitForm">
      <div>
        <label
          for="image"
          text-2xl
        >
          Upload Image:
        </label>
        <input
          ref="fileInput"
          type="file"
          @change="handleFileUpload"
        >
      </div>
      <div>
        <label for="outputFormat">Output Format:</label>
        <select v-model="outputFormat">
          <option value="jpeg">
            JPEG
          </option>
          <option value="jxl">
            JPEG-XL
          </option>
          <option value="png">
            PNG
          </option>
          <option value="webp">
            WEBP
          </option>
        </select>
      </div>
      <button
        :disabled="isSubmitDisabled"
        type="submit"
      >
        Submit
      </button>
    </form>

    <!-- Display converted image or loading indicator -->
    <div
      class="image-container"
      margin-top="20px"
      max-height="600px"
      max-width="100%"
      relative
      text-center
    >
      <img
        v-if="imageUrl"
        alt="Converted Image"
        block
        class="converted-image"
        contain
        height="auto"
        m="0 auto"
        max-height="400px"
        max-width="400px"
        :src="imageUrl"
        width="auto"
      >
      <div
        v-else-if="loading"
        absolute
        class="loading-indicator"
        left="50%"
        top="50%"
        transform="translate(-50%, -50%)"
      >
        <p>Loading...</p>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import ky from 'ky'

const selectedFile = ref<File | null>(null)
const imageUrl = ref<string | null>(null)
const outputFormat = ref<string>('png') // Default output format

const loading = ref<boolean>(false) // Loading state

const handleFileUpload = (event: Event): void => {
  const target = event.target as HTMLInputElement
  if (target.files && target.files.length > 0) {
    selectedFile.value = target.files[0]
  }
}

const isSubmitDisabled = computed(() => {
  return !selectedFile.value || loading.value
})

const submitForm = async (): Promise<void> => {
  if (!selectedFile.value) {
    alert('Please select a file!')
    return
  }

  // Set loading state to true
  loading.value = true

  const formData = new FormData()

  formData.append('file', selectedFile.value)
  formData.append(
    'output_type',
    new Blob([JSON.stringify(outputFormat.value)], {
      type: 'application/json',
    }),
  )

  try {
    const response = await ky.post(
      `/api/convert_image`,
      { body: formData },
    ).blob()

    // Create a blob URL for the image and set it as the image source
    const blob = new Blob([response], { type: response.type })
    imageUrl.value = URL.createObjectURL(blob)
  } catch (error) {
    console.error('Error converting image', error)
  } finally {
    // Reset loading state after request completes (whether success or error)
    loading.value = false
  }
}
</script>
