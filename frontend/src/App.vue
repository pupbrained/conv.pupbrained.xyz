<template>
  <div>
    <form @submit.prevent='submitForm'>
      <div>
        <label for='image'>Upload Image:</label>
        <input
            ref='fileInput'
            :accept='acceptedFileType'
            type='file'
            @change='handleFileUpload'
        />
      </div>
      <div>
        <label for='outputFormat'>Output Format:</label>
        <select v-model='outputFormat'>
          <option value='bmp'>BMP</option>
          <option value='gif'>GIF</option>
          <option value='ico'>ICO</option>
          <option value='jpeg'>JPEG</option>
          <option value='pam'>PAM</option>
          <option value='pbm'>PBM</option>
          <option value='pgm'>PGM</option>
          <option value='png'>PNG</option>
          <option value='ppm'>PPM</option>
          <option value='tga'>TGA</option>
          <option value='tiff'>TIFF</option>
          <option value='webp'>WEBP</option>
        </select>
      </div>
      <button :disabled='isSubmitDisabled' type='submit'>Submit</button>
    </form>

    <!-- Loading indicator -->
    <div v-if='loading' class='loading-indicator'>
      <p>Loading...</p>
    </div>

    <!-- Display converted image or loading indicator -->
    <div class='image-container'>
      <img v-if='imageUrl' :src='imageUrl' alt='Converted Image' class='converted-image' />
      <div v-else-if='loading' class='loading-indicator'>
        <p>Loading...</p>
      </div>
    </div>
  </div>
</template>

<script lang="ts" setup>
import { computed, ref } from 'vue'
import axios from 'axios'

const selectedFile = ref<File | null>(null)
const imageUrl = ref<string | null>(null)
const outputFormat = ref<string>('png') // Default output format
const acceptedFileType = ref<string>('image/*') // Default accepted file type

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
    const response = await axios.post<Blob>(
        `//${import.meta.env.VITE_BASE_URL}/convert_image`,
        formData,
        {
          headers: { 'Content-Type': 'multipart/form-data' },
          responseType: 'blob', // Ensure axios handles the response as a blob
        },
    )

    // Create a blob URL for the image and set it as the image source
    const blob = new Blob([response.data], { type: response.headers['content-type'] })
    imageUrl.value = URL.createObjectURL(blob)
  } catch (error) {
    console.error('Error converting image', error)
  } finally {
    // Reset loading state after request completes (whether success or error)
    loading.value = false
  }
}
</script>

<style scoped>
.image-container {
  position: relative;
  max-width: 100%;
  max-height: 600px;
  text-align: center;
  margin-top: 20px;
}

.converted-image {
  max-width: 400px;
  max-height: 400px;
  width: auto;
  height: auto;
  display: block;
  margin: 0 auto;
  object-fit: contain;
}

.loading-indicator {
  position: absolute;
  top: 50%;
  left: 50%;
  transform: translate(-50%, -50%);
}
</style>
